"""Measure strings in a cartridge's font (pixels).

usage: python tools/measure.py CART FONT TEXT [TEXT ...]   (FONT 0 or 2)
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import carts  # noqa: E402
import nftr  # noqa: E402
from textcodec import narc_files  # noqa: E402

code, font = sys.argv[1], int(sys.argv[2])
f = nftr.Font(narc_files(os.path.join(carts.load()[code]["data"], "a", "0", "2", "3"))[font])
for t in sys.argv[3:]:
    w = sum(f.advance(ord(c)) or 0 for c in t)
    missing = [c for c in t if not f.has(ord(c))]
    print(f"{w:4d}  {t!r}{'  missing ' + ''.join(missing) if missing else ''}")
