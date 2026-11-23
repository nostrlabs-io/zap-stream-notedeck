#!/usr/bin/env python3
import hashlib
import sys
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
STORE = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "store"

MAX_BYTES = 8 * 1024 * 1024
MIN_SIDE, MAX_SIDE = 320, 3840
MAX_ASPECT = 2.3
SETS = {
    "play/phone": {"size": (1080, 1920), "count": (2, 8)},
    "play/tablet-7": {"size": (1200, 1920), "count": (4, 8)},
    "play/tablet-10": {"size": (1600, 2560), "count": (4, 8)},
    "play-captioned/phone": {"size": (1080, 1920), "count": (2, 8)},
    "play-captioned/tablet-7": {"size": (1200, 1920), "count": (4, 8)},
    "play-captioned/tablet-10": {"size": (1600, 2560), "count": (4, 8)},
    "appstore/iphone-6.9": {"size": (1320, 2868), "count": (1, 10)},
    "appstore/ipad-13": {"size": (2064, 2752), "count": (1, 10)},
}


def check_set(name, spec):
    folder = STORE / name
    files = sorted(folder.glob("*.png"))
    errors = []
    if not files:
        return None
    lo, hi = spec["count"]
    if not lo <= len(files) <= hi:
        errors.append(f"{name}: {len(files)} shots, need {lo} to {hi}")
    seen = {}
    for f in files:
        img = Image.open(f)
        w, h = img.size
        if (w, h) != spec["size"]:
            errors.append(f"{f}: size {w}x{h}, expected {spec['size'][0]}x{spec['size'][1]}")
        if not (MIN_SIDE <= w <= MAX_SIDE and MIN_SIDE <= h <= MAX_SIDE):
            errors.append(f"{f}: side outside {MIN_SIDE}-{MAX_SIDE}")
        if max(w, h) / min(w, h) > MAX_ASPECT:
            errors.append(f"{f}: aspect above 2.3:1")
        if f.stat().st_size > MAX_BYTES:
            errors.append(f"{f}: {f.stat().st_size} bytes, over 8 MB")
        if img.format != "PNG":
            errors.append(f"{f}: not PNG")
        if img.mode in ("RGBA", "LA", "P") and img.mode != "P":
            errors.append(f"{f}: has an alpha channel")
        arr = np.asarray(img.convert("L"))
        if arr.max() == arr.min():
            errors.append(f"{f}: one flat colour")
        elif arr.mean() < 3 or arr.mean() > 252:
            errors.append(f"{f}: black or white image")
        digest = hashlib.sha256(arr.tobytes()).hexdigest()
        if digest in seen:
            errors.append(f"{f}: identical to {seen[digest]}")
        seen[digest] = f
    return len(files), errors


def main():
    bad = 0
    found = 0
    for name, spec in SETS.items():
        result = check_set(name, spec)
        if result is None:
            continue
        found += 1
        count, errors = result
        print(f"{name}: {count} shots, {'ok' if not errors else 'FAILED'}")
        for e in errors:
            print("  " + e)
        bad += len(errors)
    if not found:
        print("no screenshot sets found")
        sys.exit(1)
    sys.exit(1 if bad else 0)


main()
