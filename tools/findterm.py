"""Find which text file and entry hold a term, in one cartridge.

usage: python tools/findterm.py CART TERM [TERM ...]
Prints archive (2 or 3), file, entry for entries whose whole text is the term
(and, failing that, entries that contain it).
"""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
import carts  # noqa: E402
from textcodec import read_texts, plain  # noqa: E402

code = sys.argv[1]
C = carts.load()
texts = read_texts(C[code]["data"])
for term in sys.argv[2:]:
    exact, part = [], []
    for (arc, f, e), blocks in texts.items():
        s = plain(blocks[0])
        if s == term:
            exact.append((arc, f, e))
        elif term in s and len(part) < 3:
            part.append((arc, f, e, s[:60]))
    print(f"{term!r}: exact {exact[:8]}{' ...' if len(exact) > 8 else ''}; contains {part}")
