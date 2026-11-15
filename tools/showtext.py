"""Print text entries of one cartridge.

usage: python tools/showtext.py CART ARCHIVE FILE FIRST [LAST]
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import carts  # noqa: E402
from textcodec import decode, narc_files, plain  # noqa: E402

code, arc, f, a = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
b = int(sys.argv[5]) if len(sys.argv) > 5 else a
C = carts.load()
blocks = decode(narc_files(os.path.join(C[code]["data"], "a", "0", "0", str(arc)))[f])
for e in range(a, b + 1):
    print(e, " | ".join(plain(bl[e]).replace("\n", " / ") for bl in blocks))
