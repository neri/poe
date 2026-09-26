#!/usr/bin/env python3
"""Check that a VirtIO GPU scanout contains the MiniOS console."""
import argparse
import os
import shlex
import subprocess
import tempfile
import time


def main(arch, kernel, smp):
    with tempfile.TemporaryDirectory() as directory:
        serial = os.path.join(directory, "serial.log")
        screenshot = os.path.join(directory, "screen.ppm")
        disk = os.path.join(directory, "disk.img")
        with open(disk, "wb") as image:
            image.write(b"\0" * (512 * 1024))
        binary = f"qemu-system-{arch}"
        if arch == "aarch64":
            machine = ["-M", "virt", "-cpu", "cortex-a53", "-m", "512M", "-smp", str(smp)]
        elif arch == "riscv32":
            # rv32 boots through the bundled minisbi, not OpenSBI.
            machine = ["-M", "virt", "-bios", "none", "-smp", str(smp)]
        else:
            machine = ["-M", "virt", "-smp", str(smp)]
        argv = [binary, *machine, "-display", "none", "-serial", f"file:{serial}",
            "-monitor", "stdio",
            "-global", "virtio-mmio.force-legacy=false", "-kernel", kernel,
            "-object", "rng-builtin,id=rng0", "-device", "virtio-rng-device,rng=rng0",
            "-drive", f"if=none,id=vblk,format=raw,file={disk},readonly=on",
            "-device", "virtio-blk-device,drive=vblk", "-device", "virtio-gpu-device"]
        print(subprocess.check_output([binary, "--version"], text=True).splitlines()[0], flush=True)
        print(f"QEMU command: {shlex.join(argv)}", flush=True)
        proc = subprocess.Popen(argv, stdin=subprocess.PIPE,
            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 20
            while not os.path.exists(serial) or b"virtio-gpu:" not in open(serial, "rb").read():
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"GPU did not attach: {open(serial, 'rb').read()[-1000:]!r}")
                time.sleep(0.05)
            time.sleep(0.5)
            boot_log = open(serial, "rb").read()
            if (b"virtio-rng: ready" not in boot_log or b"virtio-blk:" not in boot_log
                    or b"virtio-gpu: start failed" in boot_log):
                raise RuntimeError(f"combined device init failed: {boot_log[-1000:]!r}")
            proc.stdin.write(f"screendump {screenshot}\n".encode())
            proc.stdin.flush()
            time.sleep(0.5)
            if not os.path.exists(screenshot):
                raise RuntimeError(f"screendump failed: {open(serial, 'rb').read()[-1000:]!r}")
            with open(screenshot, "rb") as image:
                header = image.readline()
                dims = image.readline()
                maxval = image.readline()
                pixels = image.read()
            if header != b"P6\n" or maxval != b"255\n" or not pixels:
                raise RuntimeError("invalid GPU screenshot")
            if len(set(pixels)) < 2:
                raise RuntimeError("GPU screenshot contains no console drawing")
            print(f"{arch}: GPU scanout {dims.decode().strip()}, {len(set(pixels))} channel values")
        finally:
            proc.terminate()
            try:
                proc.communicate(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.communicate()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--arch", choices=["aarch64", "riscv32", "riscv64"], required=True)
    parser.add_argument("--kernel", required=True)
    parser.add_argument("--smp", type=int, default=1)
    args = parser.parse_args()
    main(args.arch, args.kernel, args.smp)
