#!/bin/sh
# Patch PE optional-header OS/subsystem version to 6.1 (Windows 7 minimum).
#
# zig cc (cargo-zigbuild's linker) does not forward --major-subsystem-version
# to its internal lld-link, so the linked .exe ships with the lld default
# (6.0). This post-link step rewrites the four version fields to declare
# Windows 7 (NT 6.1) as the minimum supported system.
#
# Usage: contrib/patch-pe-version.sh path/to/kursor.exe [more.exe ...]
set -eu

for f in "$@"; do
    python3 - "$f" <<'EOF'
import struct
import sys

path = sys.argv[1]
with open(path, "r+b") as fh:
    header = fh.read(0x100)
    if header[0:2] != b"MZ":
        raise SystemExit(f"{path}: not a PE file")
    pe = struct.unpack_from("<I", header, 0x3C)[0]
    opt = pe + 24
    magic = struct.unpack_from("<H", header, opt)[0]
    if magic not in (0x10B, 0x20B):
        raise SystemExit(f"{path}: bad optional-header magic")
    fh.seek(opt + 0x28)
    fh.write(struct.pack("<H", 6))  # MajorOperatingSystemVersion
    fh.seek(opt + 0x2A)
    fh.write(struct.pack("<H", 1))  # MinorOperatingSystemVersion
    fh.seek(opt + 0x30)
    fh.write(struct.pack("<H", 6))  # MajorSubsystemVersion
    fh.seek(opt + 0x32)
    fh.write(struct.pack("<H", 1))  # MinorSubsystemVersion
print(f"{path}: OS/subsystem version set to 6.1")
EOF
done
