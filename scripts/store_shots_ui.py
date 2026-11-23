#!/usr/bin/env python3
import csv
import io
import os
import subprocess
import sys
import time

SERIAL = os.environ["ANDROID_SERIAL"]


def adb(*args, binary=False):
    out = subprocess.run(["adb", "-s", SERIAL, *args], capture_output=True, check=True).stdout
    return out if binary else out.decode()


def screencap(path):
    data = adb("exec-out", "screencap", "-p", binary=True)
    with open(path, "wb") as f:
        f.write(data)


def ocr(path):
    out = subprocess.run(
        ["tesseract", path, "-", "--psm", "11", "tsv"], capture_output=True, check=True
    ).stdout.decode()
    rows = csv.DictReader(io.StringIO(out), delimiter="\t", quoting=csv.QUOTE_NONE)
    found = []
    for r in rows:
        text = (r.get("text") or "").strip()
        if text and float(r["conf"]) > 30:
            found.append(
                (text, int(r["left"]), int(r["top"]), int(r["width"]), int(r["height"]))
            )
    return found


def words(path):
    from PIL import Image, ImageOps

    inverted = path + ".inv.png"
    ImageOps.invert(Image.open(path).convert("RGB")).save(inverted)
    return ocr(path) + ocr(inverted)


def locate(path, phrase, index=0):
    target = phrase.lower().split()
    ws = words(path)
    hits = []
    for i in range(len(ws) - len(target) + 1):
        window = ws[i : i + len(target)]
        if all(t in w[0].lower() for t, w in zip(target, window)):
            x0 = min(w[1] for w in window)
            y0 = min(w[2] for w in window)
            x1 = max(w[1] + w[3] for w in window)
            y1 = max(w[2] + w[4] for w in window)
            hits.append(((x0 + x1) // 2, (y0 + y1) // 2))
    if len(hits) > index:
        return hits[index]
    return None


def wait_for(phrase, timeout=40, index=0):
    tmp = f"/tmp/store-shots-{SERIAL}.png"
    end = time.time() + timeout
    while time.time() < end:
        screencap(tmp)
        pos = locate(tmp, phrase, index)
        if pos:
            return pos
        time.sleep(2)
    return None


def main():
    cmd = sys.argv[1]
    if cmd == "tap":
        pos = wait_for(sys.argv[2], float(sys.argv[3]) if len(sys.argv) > 3 else 40,
                       int(sys.argv[4]) if len(sys.argv) > 4 else 0)
        if not pos:
            print(f"text not found: {sys.argv[2]}", file=sys.stderr)
            sys.exit(1)
        adb("shell", "input", "tap", str(pos[0]), str(pos[1]))
    elif cmd == "wait":
        pos = wait_for(sys.argv[2], float(sys.argv[3]) if len(sys.argv) > 3 else 40)
        if not pos:
            print(f"text not found: {sys.argv[2]}", file=sys.stderr)
            sys.exit(1)
    elif cmd == "has":
        tmp = f"/tmp/store-shots-{SERIAL}.png"
        screencap(tmp)
        sys.exit(0 if locate(tmp, sys.argv[2]) else 1)
    elif cmd == "dump":
        tmp = f"/tmp/store-shots-{SERIAL}.png"
        screencap(tmp)
        for w in words(tmp):
            print(w[0], w[1], w[2])


main()
