#!/usr/bin/env python3
"""QEMU tests for the xHCI USB stack (docs/USB_HOST_RPI4_PLAN.md, stage 7),
and for USB mass storage on both stacks (docs/USB_MSC_RPI_PLAN.md, stage 5).

Boots bin/kernel.img — built with FEATURES=minios/usb_debug, since the tests
read the stack's diagnostic lines — under QEMU, drives it over QMP, and checks
the serial log.  The UART is a file, never a terminal, so nothing typed at a
keyboard can be mistaken for USB input.

    usage: usb-test.py [--kernel bin/kernel.img] [--dtb-dir dtb] [scenario ...]

With no scenario named, all of them run.  Exit status is 0 only if every
scenario passed.  Needs qemu-system-aarch64 and nothing beyond the Python
standard library.

What QEMU cannot show: the BCM2711 PCIe bring-up (QEMU's raspi4b has no PCIe),
split transactions (its only hub is Full Speed), and an endpoint that halts
while its device is still attached.  Those are checked on a Raspberry Pi 4/400.

The mass storage scenarios (msc-*) read raw blocks of a known image through
the POE shell's usbread command, typed on the UART, and compare CRC-32s.
QEMU attaches usb-storage to a SuperSpeed port when the controller has one,
so the High Speed ones use qemu-xhci with p3=0 or put the device behind a
hub, and msc-superspeed leaves the SuperSpeed ports on.  QEMU's DWC2 and devices never NAK, stall or time out a
bulk transfer on their own; those paths are covered by the unit tests
(minios/src/io/usb/class/msc/tests.rs) and on real hardware.
"""

import argparse
import json
import os
import re
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import zlib


class Machine:
    """One QEMU run with a QMP socket and a serial log."""

    def __init__(self, kernel, *qemu_args, machine="virt,gic-version=2", extra=(), uart=False):
        self.dir = tempfile.mkdtemp(prefix="usb-test-")
        self.log = os.path.join(self.dir, "serial.log")
        qmp = os.path.join(self.dir, "qmp.sock")
        server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        server.bind(qmp)
        server.listen(1)
        # With `uart`, the UART is also an input: a socket the tests type
        # into, with everything the guest sends still logged to the file.
        uart_socket = os.path.join(self.dir, "uart.sock")
        serial = (["-chardev", "socket,id=uart,path=%s,server=on,wait=off,logfile=%s"
                   % (uart_socket, self.log), "-serial", "chardev:uart"]
                  if uart else ["-serial", "file:" + self.log])
        command = ["qemu-system-aarch64", "-M", machine, "-display", "none",
                   *serial, "-kernel", kernel,
                   "-qmp", "unix:" + qmp, *extra]
        if machine.startswith("virt"):
            command += ["-cpu", "cortex-a53", "-m", "512M", "-nic", "none"]
        command += list(qemu_args)
        self.process = subprocess.Popen(command, stdout=subprocess.DEVNULL,
                                        stderr=subprocess.DEVNULL)
        server.settimeout(20)
        connection, _ = server.accept()
        self.qmp = connection.makefile("rw")
        self.qmp.readline()
        self.run("qmp_capabilities")
        self.uart = None
        if uart:
            self.uart = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self.uart.connect(uart_socket)
            # What the guest sends is in the log file already, but it also
            # has to be taken off the socket: once that fills, the UART stops
            # draining and the guest waits on it forever.
            threading.Thread(target=self._drain, daemon=True).start()

    def run(self, execute, **arguments):
        message = {"execute": execute}
        if arguments:
            message["arguments"] = arguments
        self.qmp.write(json.dumps(message) + "\n")
        self.qmp.flush()
        while True:
            reply = json.loads(self.qmp.readline())
            if "event" not in reply:
                if "error" in reply:
                    raise RuntimeError("%s: %s" % (execute, reply["error"]["desc"]))
                return reply

    def text(self):
        try:
            with open(self.log, encoding="utf-8", errors="replace") as log:
                return log.read()
        except FileNotFoundError:
            return ""

    def count(self, needle):
        return self.text().count(needle)

    def wait(self, needle, times=1, timeout=15.0):
        deadline = time.time() + timeout
        while time.time() < deadline:
            if self.count(needle) >= times:
                return True
            time.sleep(0.15)
        return False

    def press(self, key):
        for down in (True, False):
            self.run("input-send-event", events=[{
                "type": "key",
                "data": {"down": down, "key": {"type": "qcode", "data": key}}}])
            time.sleep(0.1)

    def typed(self, *keys, settle=0.6):
        """Presses `keys` and returns what reached the console, escape
        sequences stripped.  QEMU delivers keys to the keyboard added last."""
        mark = len(self.text())
        for key in keys:
            self.press(key)
        time.sleep(settle)
        return re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", self.text()[mark:])

    def _drain(self):
        try:
            while self.uart.recv(4096):
                pass
        except OSError:
            pass

    def clean(self, since=0):
        return re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", self.text()[since:])

    def boot_shell(self, text_mode=False):
        """Takes the boot menu to the shell over the UART.  On a Raspberry Pi
        the menu is on the framebuffer, so Text Mode is chosen blind — down,
        Enter, up, Enter — and the shell then appears on the UART."""
        if not text_mode:
            check(self.wait("Option", timeout=30), "the boot menu never appeared")
            time.sleep(0.5)
            self.uart.sendall(b"\r")
            check(self.wait("poe>", timeout=30), "the shell never started")
            return
        for _ in range(3):
            time.sleep(2.0)
            for key in ("\x1b[B", "\r", "\x1b[A", "\r"):
                self.uart.sendall(key.encode())
                time.sleep(0.8)
            if self.wait("poe>", timeout=15):
                return
        raise Failed("the shell never started")

    def shell(self, line, timeout=60.0):
        """Types `line` on the UART and returns what the shell printed."""
        mark = len(self.text())
        prompts = self.count("poe>")
        self.uart.sendall(line.encode() + b"\r")
        check(self.wait("poe>", prompts + 1, timeout), "no prompt after %r" % line)
        return self.clean(mark)

    def memory(self):
        return re.findall(r"xHCI memory: (\d+) KiB free, (\d+) map entries", self.text())

    def stop(self):
        if self.uart:
            self.uart.close()
        self.process.terminate()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()


class Failed(Exception):
    pass


def check(condition, message):
    if not condition:
        raise Failed(message)


def stick():
    image = os.path.join(tempfile.mkdtemp(prefix="usb-test-"), "stick.img")
    with open(image, "wb") as f:
        f.truncate(8 << 20)
    return image


def block_bytes(lba, block_size):
    """What block `lba` of a test image holds: its number, a tag, and a
    pattern that differs between blocks and within one."""
    head = struct.pack("<Q", lba) + b"LBA:BLK:"
    body = bytes(((lba * 7) + i) & 0xff for i in range(block_size - len(head)))
    return head + body


def image(blocks, block_size=512, sparse=None):
    """A raw image of `blocks` blocks.  With `sparse`, only those blocks are
    written and the rest reads as zeros, so a very large image costs nothing."""
    path = os.path.join(tempfile.mkdtemp(prefix="usb-test-"), "disk.img")
    with open(path, "wb") as f:
        if sparse is None:
            for lba in range(blocks):
                f.write(block_bytes(lba, block_size))
        else:
            f.truncate(blocks * block_size)
            for lba in sparse:
                f.seek(lba * block_size)
                f.write(block_bytes(lba, block_size))
    return path


def crc(path, block_size, lba, count):
    with open(path, "rb") as f:
        f.seek(lba * block_size)
        return "%08x" % zlib.crc32(f.read(block_size * count))


def storage_drive(path, name="stick"):
    """A block backend that outlives the device, so it can be plugged again."""
    return ["-blockdev", "driver=file,filename=%s,node-name=%s-file" % (path, name),
            "-blockdev", "driver=raw,file=%s-file,node-name=%s" % (name, name)]


def read_ok(m, path, device, lba, count, block_size=512):
    out = m.shell("usbread %d %d %d" % (device, lba, count))
    want = crc(path, block_size, lba, count)
    got = re.search(r"usb%d lba %d count %d size (\d+) crc32 ([0-9a-f]{8})" % (device, lba, count), out)
    check(got is not None, "usbread %d %d %d failed: %r" % (device, lba, count, out.strip()[-200:]))
    check(int(got.group(1)) == block_size, "block size %s, expected %d" % (got.group(1), block_size))
    check(got.group(2) == want, "lba %d+%d: crc %s, expected %s" % (lba, count, got.group(2), want))


def ready_device(m):
    """The index of the one device usbblk lists as ready."""
    out = m.shell("usbblk")
    found = re.findall(r"usb(\d+): ready", out)
    check(len(found) == 1, "not exactly one device ready: %r" % out[-300:])
    return int(found[0])


def commands(m, device=0):
    out = m.shell("usbblk")
    found = re.search(r"usb%d: cmds (\d+)" % device, out)
    check(found is not None, "usbblk did not list usb%d: %r" % (device, out[-300:]))
    return int(found.group(1))


# ---- scenarios ---------------------------------------------------------------

def direct(kernel, _dtb):
    """A keyboard on a root port drives the shell, with interrupts."""
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", "-device", "usb-kbd,bus=xhci.0", uart=True)
    try:
        check(m.wait("now feed the console", timeout=25), "keyboard never bound")
        m.boot_shell()
        out = m.typed("v", "e", "r", "ret")
        check('"ver": Bad command' in out, "the shell did not get 'ver': %r" % out[-80:])
        check(m.count("first interrupt serviced") == 1, "no interrupt was serviced")
    finally:
        m.stop()


def modifiers(kernel, _dtb):
    """Shift held across keys, and a held key that does not repeat."""
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", "-device", "usb-kbd,bus=xhci.0", uart=True)
    try:
        check(m.wait("now feed the console", timeout=25), "keyboard never bound")
        m.boot_shell()
        time.sleep(0.5)
        mark = len(m.text())
        for name, down in (("shift", True), ("a", True), ("a", False), ("b", True),
                           ("b", False), ("shift", False), ("c", True), ("c", False)):
            m.run("input-send-event", events=[{"type": "key", "data": {
                "down": down, "key": {"type": "qcode", "data": name}}}])
            time.sleep(0.15)
        m.run("input-send-event", events=[{"type": "key", "data": {
            "down": True, "key": {"type": "qcode", "data": "d"}}}])
        time.sleep(1.2)
        m.run("input-send-event", events=[{"type": "key", "data": {
            "down": False, "key": {"type": "qcode", "data": "d"}}}])
        m.press("d")
        time.sleep(1.0)
        out = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", m.text()[mark:])
        check(out.strip() == "ABcdd", "expected 'ABcdd', got %r" % out)
    finally:
        m.stop()


def root_port(kernel, _dtb):
    """The controller behind a PCIe root port, as on a Raspberry Pi 4."""
    m = Machine(kernel, "-device", "pcie-root-port,id=rp0,bus=pcie.0,chassis=1,slot=1",
                "-device", "qemu-xhci,id=xhci,bus=rp0", "-device", "usb-kbd,bus=xhci.0", uart=True)
    try:
        check(m.wait("now feed the console", timeout=25), "keyboard never bound")
        m.boot_shell()
        check("xHCI PCI: 01:00.0" in m.text(), "the controller was not found at 01:00.0")
        out = m.typed("v", "e", "r", "ret")
        check('"ver": Bad command' in out, "the shell did not get 'ver'")
        check(m.count("first interrupt serviced") == 1,
              "the swizzled interrupt was never serviced")
    finally:
        m.stop()


def replug(kernel, _dtb, cycles=20):
    """Twenty reconnects on one port; memory comes back every time."""
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", uart=True)
    try:
        check(m.wait("keyboard not found", timeout=25), "boot never finished")
        m.boot_shell()
        for cycle in range(1, cycles + 1):
            m.run("device_add", driver="usb-kbd", id="kbd", bus="xhci.0", port="1")
            check(m.wait("now feed the console", cycle, 10), "cycle %d: no bind" % cycle)
            check("k" in m.typed("k"), "cycle %d: key lost" % cycle)
            m.run("device_del", id="kbd")
            check(m.wait("keyboard detached", cycle, 10), "cycle %d: no detach" % cycle)
            time.sleep(0.2)
        memory = m.memory()
        check(len(memory) >= cycles, "memory was not reported on every detach")
        check(len(set(memory)) == 1, "memory drifted across cycles: %s" % sorted(set(memory)))
    finally:
        m.stop()


def hub_hotplug(kernel, _dtb, cycles=5):
    """A keyboard plugged in and out behind a hub."""
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", "-device", "usb-hub,id=h,bus=xhci.0",
                uart=True)
    try:
        check(m.wait("hub with", timeout=25), "hub never configured")
        m.boot_shell()
        for cycle in range(1, cycles + 1):
            m.run("device_add", driver="usb-kbd", id="kbd", bus="xhci.0", port="1.2")
            check(m.wait("hub port 2: boot keyboard", cycle, 12), "cycle %d: no bind" % cycle)
            check("k" in m.typed("k"), "cycle %d: key lost" % cycle)
            m.run("device_del", id="kbd")
            check(m.wait("hub port 2: keyboard detached", cycle, 12),
                  "cycle %d: no detach" % cycle)
            time.sleep(0.3)
    finally:
        m.stop()


def hub_pull(kernel, _dtb):
    """The hub pulled with a keyboard still in it, then a keyboard on a root port."""
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", "-device", "usb-hub,id=h,bus=xhci.0",
                "-device", "usb-kbd,id=kbd,bus=xhci.0,port=1.1", uart=True)
    try:
        check(m.wait("hub port 1: boot keyboard", timeout=25), "keyboard behind hub never bound")
        m.boot_shell()
        check("k" in m.typed("k"), "key lost before the pull")
        m.run("device_del", id="kbd")
        m.run("device_del", id="h")
        check(m.wait("hub port 1: keyboard detached", timeout=12), "pull not noticed")
        time.sleep(1.5)
        m.run("device_add", driver="usb-kbd", id="kbd2", bus="xhci.0", port="2")
        check(m.wait("boot keyboard on interface", 2, 15), "root keyboard never bound")
        check("j" in m.typed("j"), "key lost after the pull")
    finally:
        m.stop()


def multi(kernel, _dtb, layout):
    """Two keyboards at once feed the console; one leaves, the other stays."""
    if layout == "root":
        devices = ["-device", "usb-kbd,id=kbdA,bus=xhci.0,port=1",
                   "-device", "usb-kbd,id=kbdB,bus=xhci.0,port=2"]
        third = "3"
    else:
        devices = ["-device", "usb-hub,id=h,bus=xhci.0",
                   "-device", "usb-kbd,id=kbdA,bus=xhci.0,port=1.2",
                   "-device", "usb-kbd,id=kbdB,bus=xhci.0,port=1.4"]
        third = "1.3"
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", *devices, uart=True)
    try:
        check(m.wait("2 keyboard interface(s) now feed the console", timeout=25),
              "both keyboards were not bound")
        m.boot_shell()
        check("b" in m.typed("b"), "key lost on the second keyboard")
        m.run("device_del", id="kbdB")
        check(m.wait("keyboard detached", timeout=10), "detach not noticed")
        time.sleep(0.5)
        check("a" in m.typed("a"), "the first keyboard stopped working")
        m.run("device_add", driver="usb-kbd", id="kbdC", bus="xhci.0", port=third)
        check(m.wait("2 keyboard interface(s) now feed the console", 2, 10),
              "the third keyboard was not bound alongside the first")
        check("d" in m.typed("d"), "key lost on the third keyboard")
        check(", 0 failure(s)" in m.text(), "enumeration failed at boot")
    finally:
        m.stop()


def multi_root(kernel, dtb):
    multi(kernel, dtb, "root")


def multi_hub(kernel, dtb):
    multi(kernel, dtb, "hub")


def mass_storage(kernel, _dtb):
    """A mass storage device is looked at once, whatever else comes and goes."""
    image = stick()
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", "-device", "usb-hub,id=h,bus=xhci.0",
                "-drive", "if=none,id=stick,format=raw,file=" + image,
                "-device", "usb-storage,bus=xhci.0,port=1.1,drive=stick",
                "-device", "usb-kbd,id=ka,bus=xhci.0,port=1.2")
    try:
        check(m.wait("now feed the console", timeout=25), "keyboard never bound")
        for cycle in range(1, 6):
            m.run("device_add", driver="usb-kbd", id="kb", bus="xhci.0", port="1.4")
            check(m.wait("now feed the console", cycle + 1, 10), "cycle %d: no bind" % cycle)
            m.run("device_del", id="kb")
            check(m.wait("hub port 4: keyboard detached", cycle, 10),
                  "cycle %d: no detach" % cycle)
            time.sleep(0.3)
        check(m.count("46f4:0001") == 1,
              "the mass storage device was enumerated %d times" % m.count("46f4:0001"))
        memory = m.memory()
        check(len(set(memory[1:])) == 1, "memory drifted: %s" % sorted(set(memory)))
    finally:
        m.stop()


def no_room(kernel, _dtb):
    """A fifth keyboard waits for a place and takes the first one freed."""
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", "-device", "usb-hub,id=h,bus=xhci.0",
                *sum((["-device", "usb-kbd,id=k%d,bus=xhci.0,port=1.%d" % (n, n)]
                      for n in range(1, 5)), []), uart=True)
    try:
        check(m.wait("4 keyboard interface(s) now feed the console", timeout=30),
              "four keyboards were not bound")
        m.boot_shell()
        m.run("device_add", driver="usb-kbd", id="k5", bus="xhci.0", port="1.5")
        check(m.wait("but 4 are already in use", timeout=10), "the fifth was not turned away")
        m.run("device_del", id="k1")
        check(m.wait("hub port 5: boot keyboard", timeout=10),
              "the fifth did not take the freed place")
        check("z" in m.typed("z"), "key lost on the fifth keyboard")
    finally:
        m.stop()


def gicv3(kernel, _dtb):
    """Where the interrupt controller cannot route the line, polling works."""
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", "-device", "usb-kbd,bus=xhci.0",
                machine="virt,gic-version=3", uart=True)
    try:
        check(m.wait("now feed the console", timeout=25), "keyboard never bound")
        m.boot_shell()
        check("completion mode: polling" in m.text(), "did not fall back to polling")
        out = m.typed("v", "e", "r", "ret")
        check('"ver": Bad command' in out, "the shell did not get 'ver'")
    finally:
        m.stop()


def no_controller(kernel, _dtb):
    """A machine without an xHCI boots without a word from the stack."""
    m = Machine(kernel)
    try:
        check(m.wait("poe>", timeout=20) or m.wait("myosExp", timeout=5), "boot never finished")
        check("xHCI" not in m.text() and "PCI:" not in m.text(),
              "the stack printed on a machine without a controller")
    finally:
        m.stop()


def raspberry_pi_3(kernel, dtb):
    """The DWC2 path of a Raspberry Pi 3 is untouched."""
    m = Machine(kernel, "-dtb", os.path.join(dtb, "bcm2710-rpi-3-b.dtb"),
                "-device", "usb-hub,id=hub1,bus=usb-bus.0,port=1",
                "-device", "usb-kbd,id=kbd1,bus=usb-bus.0,port=1.1",
                machine="raspi3b")
    try:
        check(m.wait("Boot keyboard ready", timeout=25), "the Pi 3 keyboard never bound")
        check("xHCI" not in m.text(), "the xHCI stack ran on a Raspberry Pi 3")
    finally:
        m.stop()


def msc_root(kernel, _dtb):
    """High Speed mass storage on a root port, next to a keyboard: blocks at
    the start, middle and end, a read over several commands, writes refused
    without a command, a range check, and the keyboard still typing."""
    path = image(16384)
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci,p3=0", *storage_drive(path),
                "-device", "usb-storage,bus=xhci.0,port=1,drive=stick",
                "-device", "usb-kbd,bus=xhci.0,port=2", uart=True)
    try:
        check(m.wait("USB MSC 0: ready, 16384 blocks of 512 bytes", timeout=30), "never ready")
        check("512 bytes, burst 0/0 (High)" in m.text(), "not High Speed")
        m.boot_shell()
        for lba, count in ((0, 1), (8191, 1), (16383, 1), (100, 64), (16000, 256)):
            read_ok(m, path, 0, lba, count)
        before = commands(m)
        out = m.shell("usbwrite 0 0")
        check("Err(WriteProtected)" in out, "write not refused: %r" % out)
        out = m.shell("usbread 0 16383 2")
        check("InvalidParameter" in out, "read past the end not refused: %r" % out)
        check(commands(m) == before, "a refused request reached the device")
        check("k" in m.typed("k"), "the keyboard stopped")
    finally:
        m.stop()


def msc_hub(kernel, _dtb):
    """Full Speed mass storage behind a hub, with a keyboard on the same hub,
    on a controller that also has SuperSpeed ports."""
    path = image(4096)
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", "-device", "usb-hub,id=h,bus=xhci.0",
                *storage_drive(path),
                "-device", "usb-storage,bus=xhci.0,port=1.1,drive=stick",
                "-device", "usb-kbd,bus=xhci.0,port=1.2", uart=True)
    try:
        check(m.wait("USB MSC 0: ready, 4096 blocks", timeout=30), "never ready")
        check("64 bytes, burst 0/0 (Full)" in m.text(), "not Full Speed")
        m.boot_shell()
        read_ok(m, path, 0, 0, 1)
        read_ok(m, path, 0, 4095, 1)
        read_ok(m, path, 0, 1000, 40)
        check("j" in m.typed("j"), "the keyboard behind the same hub stopped")
    finally:
        m.stop()


def msc_superspeed(kernel, _dtb, cycles=3):
    """SuperSpeed mass storage on a USB3 root port — the blue sockets of a
    Raspberry Pi 4 — next to a keyboard on a USB 2.0 one: 1024-byte packets,
    reads, and plugged in and out with memory coming back every time."""
    path = image(16384)
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", *storage_drive(path),
                "-device", "usb-storage,id=st,bus=xhci.0,port=1,drive=stick",
                "-device", "usb-kbd,bus=xhci.0,port=2", uart=True)
    try:
        check(m.wait("USB MSC 0: ready, 16384 blocks of 512 bytes", timeout=30), "never ready")
        check("1024 bytes, burst" in m.text() and "(Super)" in m.text(), "not SuperSpeed")
        m.boot_shell()
        for lba, count in ((0, 1), (16383, 1), (100, 64), (16000, 256)):
            read_ok(m, path, 0, lba, count)
        for cycle in range(1, cycles + 1):
            m.run("device_del", id="st")
            check(m.wait(": detached", cycle, 10), "cycle %d: no detach" % cycle)
            time.sleep(0.3)
            m.run("device_add", driver="usb-storage", id="st", bus="xhci.0", port="1",
                  drive="stick")
            check(m.wait("ready, 16384 blocks", cycle + 1, 15), "cycle %d: never ready" % cycle)
            read_ok(m, path, ready_device(m), cycle * 10, cycle)
        memory = m.memory()
        check(len(memory) >= cycles, "memory was not reported on every detach")
        check(len(set(memory)) == 1, "memory drifted across cycles: %s" % sorted(set(memory)))
        check("k" in m.typed("k"), "the keyboard stopped")
    finally:
        m.stop()


def msc_4k(kernel, _dtb):
    """A device with 4 KiB logical blocks."""
    path = image(2048, 4096)
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci,p3=0", *storage_drive(path),
                "-device", "usb-storage,bus=xhci.0,port=1,drive=stick,logical_block_size=4096,physical_block_size=4096",
                uart=True)
    try:
        check(m.wait("ready, 2048 blocks of 4096 bytes", timeout=30), "never ready")
        m.boot_shell()
        read_ok(m, path, 0, 0, 1, 4096)
        read_ok(m, path, 0, 2047, 1, 4096)
        read_ok(m, path, 0, 3, 9, 4096)
    finally:
        m.stop()


def msc_large(kernel, _dtb):
    """More than 2^32 blocks: READ CAPACITY(16), and READ(16) past 2 TiB."""
    blocks = (1 << 32) + 4096
    marks = (0, 1, (1 << 32) - 1, 1 << 32, blocks - 2, blocks - 1)
    path = image(blocks, sparse=marks)
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci,p3=0", *storage_drive(path),
                "-device", "usb-storage,bus=xhci.0,port=1,drive=stick", uart=True)
    try:
        check(m.wait("ready, %d blocks of 512 bytes" % blocks, timeout=30),
              "the capacity was not read in full")
        m.boot_shell()
        read_ok(m, path, 0, 0, 2)
        read_ok(m, path, 0, (1 << 32) - 1, 2)
        read_ok(m, path, 0, blocks - 2, 2)
    finally:
        m.stop()


def msc_hotplug(kernel, _dtb, cycles=5):
    """Plugged in and out: a stale handle is refused, a new attachment reads,
    and memory comes back every time."""
    path = image(2048)
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci,p3=0", *storage_drive(path), uart=True)
    try:
        check(m.wait("keyboard not found", timeout=30), "boot never finished")
        m.boot_shell()
        for cycle in range(1, cycles + 1):
            m.run("device_add", driver="usb-storage", id="st", bus="xhci.0", port="1",
                  drive="stick")
            check(m.wait("ready, 2048 blocks", cycle, 15), "cycle %d: never ready" % cycle)
            device = ready_device(m)
            check(device == 0, "cycle %d: came back as usb%d, not usb0" % (cycle, device))
            read_ok(m, path, device, cycle * 10, cycle)
            m.run("device_del", id="st")
            check(m.wait(": detached", cycle, 10), "cycle %d: no detach" % cycle)
            out = m.shell("usbread %d 0 1" % device)
            check("NoMedia" in out, "cycle %d: read after unplug: %r" % (cycle, out[-120:]))
            time.sleep(0.3)
        memory = m.memory()
        check(len(memory) >= cycles, "memory was not reported on every detach")
        check(len(set(memory)) == 1, "memory drifted across cycles: %s" % sorted(set(memory)))
    finally:
        m.stop()


def msc_limit(kernel, _dtb):
    """A fifth mass storage interface is refused by name, the four work."""
    paths = [image(64) for _ in range(5)]
    devices = []
    for n, path in enumerate(paths):
        devices += storage_drive(path, "s%d" % n)
        devices += ["-device", "usb-storage,bus=xhci.0,port=1.%d,drive=s%d" % (n + 1, n)]
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", "-device", "usb-hub,id=h,bus=xhci.0",
                *devices, uart=True)
    try:
        check(m.wait("refused, 4 already in use", timeout=40), "the fifth was not refused")
        check(m.wait("USB MSC 3: ready", timeout=20), "four were not made ready")
        m.boot_shell()
        out = m.shell("usbblk")
        check(len(re.findall(r"usb\d: ready 64 x 512", out)) == 4, "not four ready: %r" % out)
        for device in range(4):
            got = re.search(r"usb%d: Xhci port \d+\.(\d)" % device, out)
            check(got is not None, "usb%d has no hub port" % device)
            read_ok(m, paths[int(got.group(1)) - 1], device, 63, 1)
    finally:
        m.stop()


def msc_pi3(kernel, dtb):
    """The DWC2 of a Raspberry Pi 3: Full Speed storage behind a hub, one
    packet at a time, next to the keyboard."""
    path = image(4096)
    m = Machine(kernel, "-dtb", os.path.join(dtb, "bcm2710-rpi-3-b.dtb"),
                "-device", "usb-hub,id=hub1,bus=usb-bus.0,port=1",
                "-device", "usb-kbd,id=kbd1,bus=usb-bus.0,port=1.1",
                *storage_drive(path),
                "-device", "usb-storage,bus=usb-bus.0,port=1.2,drive=stick",
                machine="raspi3b", uart=True)
    try:
        check(m.wait("USB MSC 0: ready, 4096 blocks", timeout=40), "never ready")
        check("(64 byte packets)" in m.text(), "not Full Speed")
        check("Boot keyboard ready" in m.text(), "the keyboard was not bound as well")
        m.boot_shell(text_mode=True)
        read_ok(m, path, 0, 0, 1)
        read_ok(m, path, 0, 4095, 1)
        read_ok(m, path, 0, 77, 40)
        check("m" in m.typed("m", settle=1.5), "the keyboard stopped")
    finally:
        m.stop()


def msc_pi3_root(kernel, dtb):
    """The DWC2 with High Speed storage on its root port: 512-byte packets."""
    path = image(16384)
    m = Machine(kernel, "-dtb", os.path.join(dtb, "bcm2710-rpi-3-b.dtb"),
                *storage_drive(path),
                "-device", "usb-storage,bus=usb-bus.0,port=1,drive=stick",
                machine="raspi3b", uart=True)
    try:
        check(m.wait("USB MSC 0: ready, 16384 blocks", timeout=40), "never ready")
        check("(512 byte packets)" in m.text(), "not High Speed")
        m.boot_shell(text_mode=True)
        read_ok(m, path, 0, 0, 1)
        read_ok(m, path, 0, 16383, 1)
        read_ok(m, path, 0, 1234, 100)
        before = commands(m)
        check("Err(WriteProtected)" in m.shell("usbwrite 0 5"), "write not refused")
        check(commands(m) == before, "a refused write reached the device")
    finally:
        m.stop()


def pull_during_bench(m, path, bus, port, replug_after):
    """Starts usbbench, pulls the stick part-way through, and plugs it back
    after `replug_after` seconds.  The bench has to end early on the unplug,
    and the stick has to come back and read."""
    mark = len(m.text())
    prompts = m.count("poe>")
    m.uart.sendall(b"usbbench 0 20\r")
    time.sleep(2.0)
    m.run("device_del", id="st")
    time.sleep(replug_after)
    m.run("device_add", driver="usb-storage", id="st", bus=bus, port=port, drive="stick")
    check(m.wait("poe>", prompts + 1, 15), "usbbench did not end after the unplug")
    out = m.clean(mark)
    check("usbbench: usb0" in out and "NoMedia" in out,
          "the bench did not see the unplug: %r" % out[-300:])
    check(m.wait("ready,", 2, 20), "the stick did not come back")
    read_ok(m, path, ready_device(m), 7, 3)


def msc_pull(kernel, _dtb):
    """xHCI: the stick pulled during a read loop, and plugged back."""
    path = image(16384)
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci,p3=0", *storage_drive(path),
                "-device", "usb-storage,id=st,bus=xhci.0,port=1,drive=stick", uart=True)
    try:
        check(m.wait("ready, 16384", timeout=30), "never ready")
        m.boot_shell()
        pull_during_bench(m, path, "xhci.0", "1", 0.5)
    finally:
        m.stop()


def msc_pi3_pull(kernel, dtb):
    """DWC2: the stick pulled during a read loop behind the hub, and plugged
    back before the next port scan — only the connection-change bit tells."""
    path = image(16384)
    m = Machine(kernel, "-dtb", os.path.join(dtb, "bcm2710-rpi-3-b.dtb"),
                "-device", "usb-hub,id=hub1,bus=usb-bus.0,port=1",
                "-device", "usb-kbd,id=kbd1,bus=usb-bus.0,port=1.1",
                *storage_drive(path),
                "-device", "usb-storage,id=st,bus=usb-bus.0,port=1.2,drive=stick",
                machine="raspi3b", uart=True)
    try:
        check(m.wait("USB MSC 0: ready", timeout=40), "never ready")
        m.boot_shell(text_mode=True)
        pull_during_bench(m, path, "usb-bus.0", "1.2", 0.1)
        check("m" in m.typed("m", settle=1.5), "the keyboard stopped")
    finally:
        m.stop()


SCENARIOS = {
    "direct": direct,
    "modifiers": modifiers,
    "root-port": root_port,
    "replug": replug,
    "hub-hotplug": hub_hotplug,
    "hub-pull": hub_pull,
    "multi-root": multi_root,
    "multi-hub": multi_hub,
    "mass-storage": mass_storage,
    "no-room": no_room,
    "gicv3": gicv3,
    "no-controller": no_controller,
    "raspberry-pi-3": raspberry_pi_3,
    "msc-root": msc_root,
    "msc-hub": msc_hub,
    "msc-superspeed": msc_superspeed,
    "msc-4k": msc_4k,
    "msc-large": msc_large,
    "msc-hotplug": msc_hotplug,
    "msc-limit": msc_limit,
    "msc-pi3": msc_pi3,
    "msc-pi3-root": msc_pi3_root,
    "msc-pull": msc_pull,
    "msc-pi3-pull": msc_pi3_pull,
}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--kernel", default="bin/kernel.img")
    parser.add_argument("--dtb-dir", default="dtb")
    parser.add_argument("scenario", nargs="*", help=", ".join(SCENARIOS))
    args = parser.parse_args()
    unknown = [name for name in args.scenario if name not in SCENARIOS]
    if unknown:
        parser.error("unknown scenario(s): " + ", ".join(unknown))
    names = args.scenario or list(SCENARIOS)
    failed = []
    for name in names:
        started = time.time()
        try:
            SCENARIOS[name](args.kernel, args.dtb_dir)
            print("PASS %-15s %5.1fs" % (name, time.time() - started), flush=True)
        except (Failed, RuntimeError, OSError) as error:
            failed.append(name)
            print("FAIL %-15s %5.1fs  %s" % (name, time.time() - started, error), flush=True)
    print("%d/%d passed" % (len(names) - len(failed), len(names)))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
