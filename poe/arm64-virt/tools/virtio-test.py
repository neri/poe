#!/usr/bin/env python3
"""Boot QEMU virt and exercise polling RNG and read-only block I/O."""
import argparse
import json
import os
import selectors
import shlex
import socket
import subprocess
import tempfile
import time
import zlib


def resize_disk(qmp_path, size):
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(3)
        connection.connect(qmp_path)
        stream = connection.makefile("rwb", buffering=0)
        assert "QMP" in json.loads(stream.readline()), "missing QMP greeting"
        for request in (dict(execute="qmp_capabilities"),
                        dict(execute="block_resize", arguments=dict(device="vblk", size=size))):
            stream.write(json.dumps(request).encode() + b"\n")
            while True:
                response = json.loads(stream.readline())
                if "error" in response:
                    raise RuntimeError(f"QMP resize failed: {response}")
                if "return" in response:
                    break


def run(arch, kernel, disk, commands, readonly=True, gic_version=2, devices=True,
        unknown_device=False, rng_only=False, smp=1):
    binary = f"qemu-system-{arch}"
    if arch == "aarch64":
        machine = ["-M", f"virt,gic-version={gic_version}", "-cpu", "cortex-a53", "-m", "512M", "-smp", str(smp)]
    elif arch == "riscv32":
        # rv32 boots through the bundled minisbi, not OpenSBI.
        machine = ["-M", "virt", "-bios", "none", "-smp", str(smp)]
    else:
        machine = ["-M", "virt", "-smp", str(smp)]
    argv = [binary, *machine, "-nographic", "-monitor", "none",
            "-global", "virtio-mmio.force-legacy=false", "-kernel", kernel]
    if devices or rng_only:
        argv += ["-object", "rng-builtin,id=rng0", "-device", "virtio-rng-device,rng=rng0"]
    if devices:
        argv += ["-drive", f"if=none,id=vblk,format=raw,file={disk},readonly={'on' if readonly else 'off'}",
                 "-device", "virtio-blk-device,drive=vblk"]
    if unknown_device:
        argv += ["-device", "virtio-balloon-device"]
    qmp_path = None
    if any(command is None for command, _ in commands):
        qmp_path = os.path.join(os.path.dirname(disk), "qmp.sock")
        argv += ["-qmp", f"unix:{qmp_path},server=on,wait=off"]
    print(f"QEMU command: {shlex.join(argv)}", flush=True)
    proc = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, bufsize=0)
    selector = selectors.DefaultSelector()
    selector.register(proc.stdout, selectors.EVENT_READ)
    output = bytearray()
    try:
        for command, expected in commands:
            if command is None:
                resize_disk(qmp_path, expected)
                time.sleep(0.1)
                continue
            deadline = time.monotonic() + 20
            while b"poe>" not in output:
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"boot timeout: {output[-1000:]!r}")
                for key, _ in selector.select(0.1):
                    chunk = os.read(key.fd, 4096)
                    if not chunk:
                        raise RuntimeError(f"QEMU exited: {output[-1000:]!r}")
                    output.extend(chunk)
            output.clear()
            proc.stdin.write(command.encode() + b"\n")
            proc.stdin.flush()
            deadline = time.monotonic() + 10
            while expected.encode() not in output:
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"{command}: {output[-1000:]!r}")
                for key, _ in selector.select(0.1):
                    chunk = os.read(key.fd, 4096)
                    if not chunk:
                        raise RuntimeError(f"QEMU exited: {output[-1000:]!r}")
                    output.extend(chunk)
        label = ('read-only' if readonly else 'writable') if devices else 'no-device'
        if unknown_device:
            label = 'unknown-device'
        if rng_only:
            label = 'rng-interrupt' if arch == 'aarch64' else 'rng-polling'
        print(f"{arch}: {label} virtio run passed")
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--arch", choices=["aarch64", "riscv32", "riscv64"], required=True)
    parser.add_argument("--kernel", required=True)
    parser.add_argument("--gic-version", type=int, choices=[2, 3], default=2)
    parser.add_argument("--smp", type=int, default=1)
    args = parser.parse_args()
    binary = f"qemu-system-{args.arch}"
    irq_expected = "(active)" if args.arch == "aarch64" else "virq:"
    print(subprocess.check_output([binary, "--version"], text=True).splitlines()[0], flush=True)
    with tempfile.TemporaryDirectory() as directory:
        disk = os.path.join(directory, "disk.img")
        with open(disk, "wb") as image:
            image.write(bytes(range(256)) * 2048)
        pattern_crc = zlib.crc32(bytes(range(256)) * 600)
        written_crc = zlib.crc32(b"\x5a" * (300 * 512))
        run(args.arch, args.kernel, disk, [
            ("vrng", "vrng: no rng"),
            ("vread 0 0", "vread: no disk"),
        ], gic_version=args.gic_version, devices=False, smp=args.smp)
        run(args.arch, args.kernel, disk, [
            ("vrng", "vrng: no rng"),
            ("vread 0 0", "vread: no disk"),
        ], gic_version=args.gic_version, devices=False, unknown_device=True, smp=args.smp)
        run(args.arch, args.kernel, disk,
            [("vrng", "vrng: [")] * 16 + [("virq", irq_expected)],
            gic_version=args.gic_version, devices=False, rng_only=True, smp=args.smp)
        run(args.arch, args.kernel, disk, [
            ("vrng", "vrng: ["),
            ("vblk", "vblk0: 1024 blocks x 512 bytes"),
            ("vread 0 0", "vread: 512 bytes first [00, 01, 02, 03"),
            ("vread 0 0 300", f"crc32 {pattern_crc:08x}"),
            ("vread 0 1023", "vread: 512 bytes first [00, 01, 02, 03"),
            ("vread 0 1024", "InvalidParameter"),
            ("vwrite 0 0", "WriteProtected"),
            ("vreset 0", "vreset: Ok(())"),
            ("vread 0 0", "vread: 512 bytes first [00, 01, 02, 03"),
        ] + [("vrng", "vrng: [")] * 130 + [("virq", irq_expected)],
            gic_version=args.gic_version, smp=args.smp)
        run(args.arch, args.kernel, disk, [
            ("vwrite 0 1 300", "vwrite: 153600 bytes"),
            ("vflush 0", "vflush: Ok(())"),
            ("vread 0 1 300", f"crc32 {written_crc:08x}"),
            (None, 1024 * 1024),
            ("vread 0 1", "DeviceError"),
            ("vreset 0", "vreset: Ok(())"),
            ("vblk", "vblk0: 2048 blocks x 512 bytes"),
            ("vread 0 1024", "vread: 512 bytes first [00, 00"),
        ], readonly=False, gic_version=args.gic_version, smp=args.smp)
        with open(disk, "rb") as image:
            image.seek(512)
            assert image.read(300 * 512) == b"\x5a" * (300 * 512), "write did not reach the host image"
