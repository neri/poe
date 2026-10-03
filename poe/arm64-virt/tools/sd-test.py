#!/usr/bin/env python3
"""QEMU SD protocol tests for Raspberry Pi 3/4 using disposable raw images.

Build the diagnostic shell first with:
    make FEATURES=minios/sd,minios/usb_debug bin/kernel.img
Then run:
    python3 tools/sd-test.py --kernel bin/kernel.img [scenario ...]

Each scenario starts a fresh QEMU machine with a sparse 64 MiB SDSC or 4 GiB
SDHC image, checks an initial read, runs the explicitly destructive sdtest on
that temporary image, checks the resulting host-side bytes, and verifies the
card can be reinitialized and read again. Raspberry Pi 4 requires QEMU 9.0+.
This does not verify the Raspberry Pi 3 physical pin routing or any RISC-V
board's host controller.
"""

import argparse
import importlib.util
import os
import re
import tempfile
import time
import zlib


def load_usb_harness():
    path = os.path.join(os.path.dirname(__file__), "usb-test.py")
    spec = importlib.util.spec_from_file_location("usb_test_harness", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


usb_harness = load_usb_harness()
Machine = usb_harness.Machine
Failed = usb_harness.Failed
check = usb_harness.check


def block_bytes(lba, size=512):
    head = lba.to_bytes(8, "little") + b"SD-TEST:"
    return head + bytes(((lba * 11) + i) & 0xff for i in range(size - len(head)))


def check_image(machine, disk, image_bytes, lba, count, transformed):
    """Pause the guest and verify every byte, including unintended writes.

    directsync makes completed guest writes visible to this independent reader.
    Checking the first pass is essential: two XOR passes could hide an alias.
    """
    machine.run("stop")
    try:
        chunk_size = 1 << 20
        with open(disk, "rb") as image:
            for start in range(0, image_bytes, chunk_size):
                size = min(chunk_size, image_bytes - start)
                expected = bytearray(size)
                for block in range(lba - 1, lba + count + 1):
                    offset = block * 512 - start
                    if 0 <= offset < size:
                        data = block_bytes(block)
                        if transformed and lba <= block < lba + count:
                            data = bytes(byte ^ (0x80 | (block & 0x7f)) for byte in data)
                        expected[offset:offset + 512] = data
                actual = image.read(size)
                if actual != expected:
                    offset = next((i for i, (a, b) in enumerate(zip(actual, expected))
                                   if a != b), len(actual))
                    raise Failed("host image mismatch at byte %d (LBA %d), pass %d" %
                                 (start + offset, (start + offset) // 512,
                                  1 if transformed else 2))
    finally:
        machine.run("cont")


def usb_read_ok(machine, disk, device, lba, count):
    out = machine.shell("usbread %d %d %d" % (device, lba, count))
    match = re.search(r"usbread: usb%d lba %d count %d block_size 512 bytes %d crc32 ([0-9a-f]{8})" %
                      (device, lba, count, count * 512), out)
    expected = usb_harness.crc(disk, 512, lba, count)
    check(match is not None and match.group(1) == expected,
          "USB read failed during SD coexistence: %r" % out[-300:])


def scenario(kernel, dtb_dir, board, kind, usb=None):
    if kind == "sdsc":
        image_bytes = 64 << 20
        lba = 0x7fff
    else:
        image_bytes = 4 << 30
        lba = 0x400001
    count = 18
    with tempfile.TemporaryDirectory(prefix="sd-test-") as temp_dir:
        disk = os.path.join(temp_dir, "%s-%s.img" % (board, kind))
        with open(disk, "wb") as f:
            f.truncate(image_bytes)
            for block in range(lba - 1, lba + count + 1):
                f.seek(block * 512)
                f.write(block_bytes(block))

        usb_args = []
        usb_disk = os.path.join(temp_dir, "usb.img")
        if usb:
            with open(usb_disk, "wb") as f:
                for block in range(4096):
                    f.write(block_bytes(block))
            usb_args = usb_harness.storage_drive(usb_disk)
            if usb == "hub":
                usb_args += ["-device", "usb-hub,id=hub1,bus=usb-bus.0,port=1",
                             "-device", "usb-kbd,id=kbd1,bus=usb-bus.0,port=1.1"]
            usb_args += ["-device", "usb-storage,drive=stick,bus=usb-bus.0,port=" +
                         ("1.2" if usb == "hub" else "1")]

        dtb = os.path.join(dtb_dir, "bcm2710-rpi-3-b.dtb" if board == "rpi3"
                           else "bcm2711-rpi-4-b.dtb")
        machine_type = "raspi3b" if board == "rpi3" else "raspi4b"
        drive = "format=raw,cache=directsync,file=" + disk
        # QEMU 11.1.2 attaches -drive if=sd to the GPIO/legacy SDHCI bus.
        # The first generic-sdhci on raspi4b is the dedicated EMMC2 host.

        sd_args = (["-drive", "if=sd," + drive] if board == "rpi3" else
                   ["-drive", "if=none,id=sdtest," + drive,
                    "-device", "sd-card,drive=sdtest,bus=/generic-sdhci/sd-bus"])
        m = Machine(
            kernel,
            "-dtb", dtb,
            *sd_args,
            *usb_args,
            machine=machine_type,
            uart=True,
        )
        try:
            # POE now starts directly in the graphical shell. Enter its mode
            # command before navigating the menu to the UART text console.
            time.sleep(3)
            m.uart.sendall(b"mode\r")
            time.sleep(1)
            for key in (b"\x1b[B", b"\r", b"\x1b[A", b"\r"):
                m.uart.sendall(key)
                time.sleep(0.8)
            check(m.wait("poe>", timeout=15), "the UART shell never started")
            if usb:
                check(m.wait("USB MSC 0: ready, 4096 blocks", timeout=40),
                      "USB storage did not become ready alongside SD")
                packet = "64" if usb == "hub" else "512"
                check("(%s byte packets)" % packet in m.text(),
                      "unexpected USB storage speed")
                if usb == "hub":
                    check("Boot keyboard ready" in m.text(), "USB keyboard not initialized")
                usb_device = usb_harness.ready_device(m)
                usb_read_ok(m, usb_disk, usb_device, 77, 40)
            listing = m.shell("sdblk")
            expected_kind = "Sdsc" if kind == "sdsc" else "Sdhc"
            check("sd0: " + expected_kind in listing,
                  "unexpected SD card kind: %r" % listing[-300:])
            check("%d x 512 blocks" % (image_bytes // 512) in listing,
                  "unexpected SD capacity: %r" % listing[-300:])

            out = m.shell("sdread 0 %d %d" % (lba, count))
            expected_crc = "%08x" % zlib.crc32(
                b"".join(block_bytes(block) for block in range(lba, lba + count))
            )
            read_line = re.search(
                r"sdread: sd0 lba %d count %d block_size 512 crc32 ([0-9a-f]{8})" % (lba, count),
                out,
            )
            check(read_line is not None, "initial sdread failed: %r" % out[-300:])
            check(read_line.group(1) == expected_crc,
                  "initial image CRC %s, expected %s" % (read_line.group(1), expected_crc))

            out = m.shell("sdtest 0 %d %d" % (lba, count))
            check("would overwrite lba %d..%d" % (lba, lba + count) in out,
                  "sdtest without --destroy did not refuse writes: %r" % out[-300:])
            out = m.shell("sdread 0 %d %d" % (lba, count))
            read_line = re.search(
                r"sdread: sd0 lba %d count %d block_size 512 crc32 ([0-9a-f]{8})" % (lba, count),
                out,
            )
            check(read_line is not None and read_line.group(1) == expected_crc,
                  "non-destructive sdtest changed the image: %r" % out[-300:])

            for command, message in (
                ("sdread 0 %d 1" % (image_bytes // 512), "range is outside the card"),
                ("sdread 0 18446744073709551615 2", "range is outside the card"),
                ("sdtest 0 18446744073709551615 2 --destroy", "LBA range overflows"),
                ("sdtest 0 0 0 --destroy", "range must be nonempty"),
            ):
                out = m.shell(command, timeout=10)
                check(message in out, "invalid range accepted: %r" % out[-300:])

            out = m.shell("sdtest 0 %d %d --destroy" % (lba, count))
            check("sdtest: sd0 PASS lba %d count %d" % (lba, count) in out,
                  "sdtest failed: %r" % out[-300:])
            check_image(m, disk, image_bytes, lba, count, transformed=True)
            if usb:
                usb_read_ok(m, usb_disk, usb_device, 0, 1)
            out = m.shell("sdreset 0")
            check("sdreset: sd0 reinitialized" in out,
                  "sdreset failed: %r" % out[-300:])
            out = m.shell("sdread 0 %d %d" % (lba, count))
            reread_line = re.search(
                r"sdread: sd0 lba %d count %d block_size 512 crc32 ([0-9a-f]{8})" % (lba, count),
                out,
            )
            expected_after = b"".join(
                bytes(byte ^ (0x80 | (block & 0x7f)) for byte in block_bytes(block))
                for block in range(lba, lba + count)
            )
            check(reread_line is not None, "read after reset failed: %r" % out[-300:])
            check(reread_line.group(1) == "%08x" % zlib.crc32(expected_after),
                  "read after reset returned unexpected data: %r" % out[-300:])

            out = m.shell("sdtest 0 %d %d --destroy" % (lba, count))
            check("sdtest: sd0 PASS lba %d count %d" % (lba, count) in out,
                  "second sdtest failed: %r" % out[-300:])
            out = m.shell("sdreset 0")
            check("sdreset: sd0 reinitialized" in out,
                  "second sdreset failed: %r" % out[-300:])
            out = m.shell("sdread 0 %d %d" % (lba, count))
            reread_line = re.search(
                r"sdread: sd0 lba %d count %d block_size 512 crc32 ([0-9a-f]{8})" % (lba, count),
                out,
            )
            check(reread_line is not None and reread_line.group(1) == expected_crc,
                  "second sdtest did not restore original data: %r" % out[-300:])
            check_image(m, disk, image_bytes, lba, count, transformed=False)
            if usb:
                usb_read_ok(m, usb_disk, usb_device, 4095, 1)
                usb_read_ok(m, usb_disk, usb_device, 77, 40)
                if usb == "hub":
                    check("m" in m.typed("m", settle=1.5), "USB keyboard stopped after SD I/O")
                    m.press("backspace")
                    check("PLATFORM" in m.shell("about"), "shell stopped after USB keyboard input")
                with open(usb_disk, "rb") as f:
                    check(f.read() == b"".join(block_bytes(block) for block in range(4096)),
                          "SD writes changed the USB image")
        except (Failed, RuntimeError):
            print("Guest log (%s %s):\n%s" % (board, kind, m.clean()[-6000:]), flush=True)
            raise
        finally:
            m.stop()

    print("PASS %s %s%s" % (board, kind, " usb-" + usb if usb else ""), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel", default="bin/kernel.img")
    parser.add_argument("--dtb-dir", default="dtb")
    valid = ["rpi3-sdsc", "rpi3-sdhc", "rpi4-sdsc", "rpi4-sdhc",
             "rpi3-sdsc-usb-root", "rpi3-sdhc-usb-root",
             "rpi3-sdsc-usb-hub", "rpi3-sdhc-usb-hub"]
    parser.add_argument("scenarios", nargs="*", metavar="scenario")
    args = parser.parse_args()
    scenarios = args.scenarios or valid
    for item in scenarios:
        if item not in valid:
            parser.error("unknown scenario %r (choose from %s)" % (item, ", ".join(valid)))
    try:
        for item in scenarios:
            parts = item.split("-")
            scenario(args.kernel, args.dtb_dir, parts[0], parts[1],
                     usb=parts[3] if len(parts) == 4 else None)
    except (Failed, OSError, RuntimeError) as error:
        raise SystemExit("FAIL: %s" % error)


if __name__ == "__main__":
    main()
