"""Search a cartridge's story and system text for a pattern (a regex).

usage: python tools/grepgame.py CART PATTERN [max]
"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import carts  # noqa: E402
from textcodec import plain, read_texts  # noqa: E402

code, pat = sys.argv[1], re.compile(sys.argv[2])
mx = int(sys.argv[3]) if len(sys.argv) > 3 else 5
n = 0
for (arc, f, e), blocks in sorted(read_texts(carts.load()[code]["data"]).items()):
    s = plain(blocks[-1])
    if pat.search(s):
        print(f"{arc}/{f}/{e}: {s[:110]!r}")
        n += 1
        if n >= mx:
            break
