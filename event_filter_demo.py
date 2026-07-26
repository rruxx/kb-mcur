#!/usr/bin/env python3
"""Scan /dev/input/event* devices and filter them with kb-mcur's is_keyboard() logic."""

import struct, fcntl, os

# ── ioctl helpers ────────────────────────────────────────────────────

def _ioc(dir_, type_, nr, size):
    return (dir_ << 30) | (ord(type_) << 8) | nr | (size << 16)

IOC_READ = 2

def EVIOCGNAME(length):
    return _ioc(IOC_READ, 'E', 0x06, length)

def EVIOCGBIT(ev, length):
    return _ioc(IOC_READ, 'E', 0x20 + ev, length)

# ── filter ───────────────────────────────────────────────────────────

MIN_KEY_COUNT = 20
KEY_A   = 30
KEY_KP1 = 79
KEY_UP  = 103

def is_keyboard(path):
    try:
        fd = os.open(path, os.O_RDONLY)
    except OSError:
        return None  # can't open

    # Name
    try:
        buf = bytearray(80)
        fcntl.ioctl(fd, EVIOCGNAME(80), buf)
        name = buf.rstrip(b'\x00').decode('utf-8', errors='replace')
    except:
        name = "(unknown)"

    # EV_KEY bits
    try:
        bits = bytearray(96)
        fcntl.ioctl(fd, EVIOCGBIT(1, 96), bits)
    except OSError:
        os.close(fd)
        return (name, 0, False, False, False, "❌ no EV_KEY")

    has_a   = bool(bits[KEY_A   // 8] & (1 << (KEY_A   % 8)))
    has_kp1 = bool(bits[KEY_KP1 // 8] & (1 << (KEY_KP1 % 8)))
    has_up  = bool(bits[KEY_UP  // 8] & (1 << (KEY_UP  % 8)))

    # Count total keycodes 1..255
    count = sum(
        1 for i in range(1, 256)
        if bits[i // 8] & (1 << (i % 8))
    )

    os.close(fd)

    # Current rule: KEY_A || KEY_KP1 || KEY_UP
    passes_old = has_a or has_kp1 or has_up
    # New rule: old rule AND count >= MIN_KEY_COUNT
    passes_new = passes_old and count >= MIN_KEY_COUNT

    verdict = "✅ grab" if passes_new else ("⚠️  old" if passes_old else "✗ skip")
    return (name, count, has_a, has_kp1, has_up, verdict)

# ── main ─────────────────────────────────────────────────────────────

devices = sorted(
    (f for f in os.listdir('/dev/input') if f.startswith('event')),
    key=lambda n: int(n[5:])
)

print(f"{'Device':<12} {'Name':<42} {'keys':>5}  A  KP1  UP  verdict")
print("-" * 90)

keyboards = []
for dev_name in devices:
    path = f'/dev/input/{dev_name}'
    result = is_keyboard(path)
    if result is None:
        continue
    name, count, a, kp1, up, verdict = result
    a_s   = ' ✓' if a   else ' -'
    kp1_s = ' ✓' if kp1 else ' -'
    up_s  = ' ✓' if up  else ' -'
    print(f'{dev_name:<12} {name:<42} {count:>5} {a_s} {kp1_s} {up_s}  {verdict}')

    if "grab" in verdict or "old" in verdict:
        keyboards.append(dev_name)

print()
print(f"Would grab: {' '.join(keyboards) if keyboards else '(none)'}")
