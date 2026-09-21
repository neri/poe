#!/usr/bin/env python3
"""QEMU tests for the xHCI USB stack (docs/USB_HOST_RPI4_PLAN.md, stage 7).

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
"""

import argparse
import json
import os
import re
import socket
import subprocess
import sys
import tempfile
import time


class Machine:
    """One QEMU run with a QMP socket and a serial log."""

    def __init__(self, kernel, *qemu_args, machine="virt,gic-version=2", extra=()):
        self.dir = tempfile.mkdtemp(prefix="usb-test-")
        self.log = os.path.join(self.dir, "serial.log")
        qmp = os.path.join(self.dir, "qmp.sock")
        server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        server.bind(qmp)
        server.listen(1)
        command = ["qemu-system-aarch64", "-M", machine, "-display", "none",
                   "-serial", "file:" + self.log, "-kernel", kernel,
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

    def memory(self):
        return re.findall(r"xHCI memory: (\d+) KiB free, (\d+) map entries", self.text())

    def stop(self):
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


# ---- scenarios ---------------------------------------------------------------

def direct(kernel, _dtb):
    """A keyboard on a root port drives the shell, with interrupts."""
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", "-device", "usb-kbd,bus=xhci.0")
    try:
        check(m.wait("now feed the console", timeout=25), "keyboard never bound")
        out = m.typed("v", "e", "r", "ret")
        check('"ver": Bad command' in out, "the shell did not get 'ver': %r" % out[-80:])
        check(m.count("first interrupt serviced") == 1, "no interrupt was serviced")
    finally:
        m.stop()


def modifiers(kernel, _dtb):
    """Shift held across keys, and a held key that does not repeat."""
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", "-device", "usb-kbd,bus=xhci.0")
    try:
        check(m.wait("now feed the console", timeout=25), "keyboard never bound")
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
                "-device", "qemu-xhci,id=xhci,bus=rp0", "-device", "usb-kbd,bus=xhci.0")
    try:
        check(m.wait("now feed the console", timeout=25), "keyboard never bound")
        check("xHCI PCI: 01:00.0" in m.text(), "the controller was not found at 01:00.0")
        out = m.typed("v", "e", "r", "ret")
        check('"ver": Bad command' in out, "the shell did not get 'ver'")
        check(m.count("first interrupt serviced") == 1,
              "the swizzled interrupt was never serviced")
    finally:
        m.stop()


def replug(kernel, _dtb, cycles=20):
    """Twenty reconnects on one port; memory comes back every time."""
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci")
    try:
        check(m.wait("keyboard not found", timeout=25), "boot never finished")
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
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", "-device", "usb-hub,id=h,bus=xhci.0")
    try:
        check(m.wait("hub with", timeout=25), "hub never configured")
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
                "-device", "usb-kbd,id=kbd,bus=xhci.0,port=1.1")
    try:
        check(m.wait("hub port 1: boot keyboard", timeout=25), "keyboard behind hub never bound")
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
    m = Machine(kernel, "-device", "qemu-xhci,id=xhci", *devices)
    try:
        check(m.wait("2 keyboard interface(s) now feed the console", timeout=25),
              "both keyboards were not bound")
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
                      for n in range(1, 5)), []))
    try:
        check(m.wait("4 keyboard interface(s) now feed the console", timeout=30),
              "four keyboards were not bound")
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
                machine="virt,gic-version=3")
    try:
        check(m.wait("now feed the console", timeout=25), "keyboard never bound")
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
