#!/usr/bin/env bash
# Write a kernel partition image (kpart) to a USB memory / SD card,
# so that depthcharge can boot it with Ctrl+U in developer mode.
#
# usage: write-kpart.sh <kpart> <target>
#   target: whole removable disk (e.g. /dev/sdb), or a disk image file (*.img)
#
# The target is erased: a GPT with one ChromeOS kernel partition is created.
# Environment: CGPT, VBUTIL_KERNEL, DEVKEYS
set -eu

KPART=$1
TARGET=$2
CGPT=${CGPT:-cgpt}
VBUTIL_KERNEL=${VBUTIL_KERNEL:-vbutil_kernel}
DEVKEYS=${DEVKEYS:-/usr/share/vboot/devkeys}

# Kernel partition: 32MB from sector 2048
PART_START=2048
PART_SIZE=65536
IMAGE_SIZE=64M

die() {
    echo "error: $*" >&2
    exit 1
}

command -v "$CGPT" >/dev/null || die "cgpt not found (sudo apt install cgpt)"
command -v "$VBUTIL_KERNEL" >/dev/null || die "vbutil_kernel not found (sudo apt install vboot-kernel-utils)"
[ -f "$KPART" ] || die "$KPART not found"
[ "$(stat -c %s "$KPART")" -le $((PART_SIZE * 512)) ] || die "$KPART is larger than the partition"

case "$TARGET" in
/dev/*)
    [ -b "$TARGET" ] || die "$TARGET is not a block device.
If it is a regular file created by a mistaken dd (the partition node did not exist yet),
remove it with 'sudo rm $TARGET' and re-insert the disk." ;;
esac

if [ -b "$TARGET" ]; then
    # Block device: check that it is a removable whole disk before touching it
    SUDO=sudo
    [ "$(lsblk -dno TYPE "$TARGET")" = disk ] ||
        die "$TARGET is not a whole disk. Specify the disk, e.g. /dev/sdb instead of /dev/sdb1"
    name=$(lsblk -dno NAME "$TARGET")
    root_disk=$(lsblk -no PKNAME "$(findmnt -no SOURCE /)" 2>/dev/null || true)
    [ "$name" != "$root_disk" ] || die "$TARGET holds the root filesystem"
    removable=$(lsblk -dno RM "$TARGET" | tr -d ' ')
    transport=$(lsblk -dno TRAN "$TARGET" | tr -d ' ')
    [ "$removable" = 1 ] || [ "$transport" = usb ] ||
        die "$TARGET is not a removable disk (RM=$removable, TRAN=${transport:-none})"
    [ "$(lsblk -dno LOG-SEC "$TARGET" | tr -d ' ')" = 512 ] || die "$TARGET does not use 512-byte sectors"
    mounts=$(lsblk -nro MOUNTPOINT "$TARGET" | grep -v '^$' || true)
    for m in $mounts; do
        case "$m" in
        /media/* | /run/media/*) ;;
        *) die "$TARGET has a partition mounted at $m" ;;
        esac
    done

    echo "Target: $TARGET"
    lsblk -o NAME,SIZE,TYPE,TRAN,RM,VENDOR,MODEL,FSTYPE,LABEL,MOUNTPOINT "$TARGET"
    echo
    echo "ALL DATA ON $TARGET ($(lsblk -dno SIZE "$TARGET" | tr -d ' '), $(lsblk -dno MODEL "$TARGET" | sed 's/ *$//')) WILL BE ERASED."
    read -r -p "Type 'yes' to continue: " answer </dev/tty || answer=
    [ "$answer" = yes ] || die "aborted"

    for part in $(lsblk -nro PATH,MOUNTPOINT "$TARGET" | awk '$2 != "" {print $1}'); do
        $SUDO umount "$part"
    done
else
    # Disk image file
    SUDO=
    case "$TARGET" in
    *.img) ;;
    *) die "$TARGET is neither a block device nor a *.img file" ;;
    esac
    rm -f "$TARGET"
    truncate -s $IMAGE_SIZE "$TARGET"
fi

# Partition table and kernel partition
# (cgpt warns about the old invalid GPT before creating a new one, so show its output only on failure)
out=$($SUDO "$CGPT" create "$TARGET" 2>&1) || die "cgpt create failed: $out"
$SUDO "$CGPT" add -i 1 -t kernel -b $PART_START -s $PART_SIZE -l POE -P 10 -T 5 -S 1 "$TARGET"
$SUDO "$CGPT" boot -p "$TARGET" >/dev/null
start=$($SUDO "$CGPT" show -i 1 -b "$TARGET")
[ "$start" = $PART_START ] || die "unexpected partition start: $start"

# Write the kpart at the partition start
$SUDO dd if="$KPART" of="$TARGET" bs=512 seek="$start" conv=fsync,notrunc status=none
sync

# Verify: read back, compare, check the magic and the signature
readback=$(mktemp)
trap 'rm -f "$readback"' EXIT
# (redirected by this user: root may not write to a file of another user in /tmp, fs.protected_regular)
$SUDO dd if="$TARGET" bs=512 skip="$start" count=$(((($(stat -c %s "$KPART") + 511) / 512))) status=none >"$readback"
truncate -s "$(stat -c %s "$KPART")" "$readback"
cmp -s "$KPART" "$readback" || die "read-back data differs from $KPART"
[ "$(head -c 8 "$readback")" = CHROMEOS ] || die "the partition does not start with CHROMEOS"
"$VBUTIL_KERNEL" --verify "$readback" --signpubkey "$DEVKEYS/kernel_subkey.vbpubk" >/dev/null ||
    die "signature verification failed"

echo
$SUDO "$CGPT" show "$TARGET"
echo
echo "OK: $TARGET is ready (partition 1 starts with CHROMEOS, signature verified)."
if [ -b "$TARGET" ]; then
    echo "Remove it, insert it into the Chromebook, wait a few seconds at the warning screen, then press Ctrl+U."
fi
