#!/usr/bin/env python3
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
STORE = ROOT / "store"
FONT = ROOT / "assets/fonts/Bricolage-Bold.ttf"
BG = (13, 13, 15)
TEXT = (255, 255, 255)
ACCENT = (255, 176, 0)

CAPTIONS = {
    "01-home": "Live streams, right now",
    "02-stream-chat": "Watch and chat live",
    "03-chat-zaps": "See zaps and reactions",
    "04-landscape-player": "Full screen video",
    "05-categories": "Browse by category",
    "06-category": "Find your kind of stream",
    "07-search": "Search streams and people",
    "08-profile": "Follow your favourite streamers",
    "09-japanese": "Available in 22 languages",
    "10-settings": "Settings and safety tools",
}


def fit_font(draw, text, width, start):
    size = start
    while size > 20:
        font = ImageFont.truetype(str(FONT), size)
        if draw.textlength(text, font=font) <= width:
            return font
        size -= 2
    return ImageFont.truetype(str(FONT), 20)


def caption(src, dst, text):
    shot = Image.open(src).convert("RGB")
    w, h = shot.size
    canvas = Image.new("RGB", (w, h), BG)
    draw = ImageDraw.Draw(canvas)
    band = int(h * 0.13) if h >= w else int(h * 0.2)
    font = fit_font(draw, text, int(w * 0.88), int(band * 0.5))
    box = draw.textbbox((0, 0), text, font=font)
    draw.text(((w - box[2]) / 2, (band - box[3]) / 2), text, font=font, fill=TEXT)
    draw.rectangle([w * 0.4, band - 10, w * 0.6, band - 6], fill=ACCENT)
    inner_h = h - band
    scale = inner_h / h
    small = shot.resize((int(w * scale), inner_h), Image.LANCZOS)
    canvas.paste(small, ((w - small.width) // 2, band))
    canvas.save(dst, optimize=True)


def main():
    for src in sorted((STORE / "play").glob("*/*.png")):
        stem = src.stem
        if stem not in CAPTIONS:
            continue
        dst_dir = STORE / "play-captioned" / src.parent.name
        dst_dir.mkdir(parents=True, exist_ok=True)
        caption(src, dst_dir / src.name, CAPTIONS[stem])
        print("captioned", dst_dir / src.name)


main()
