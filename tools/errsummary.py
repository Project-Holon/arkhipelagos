"""Summarise decode errors per cartridge and kind; show a few examples.

usage: python tools/errsummary.py [CART]
"""
import sys
from collections import Counter

import scriptdis as S

only = sys.argv[1] if len(sys.argv) > 1 else None
for code, files in S.corpus():
    if only and code != only:
        continue
    fam = S.fam_of(code)
    t = S.Table(fam)
    j, dm = S.ops_for(fam)
    kinds = Counter()
    examples = []
    for i, f in enumerate(files):
        if S.kind_of(f) != "script":
            continue
        ins, bl, errs, h = S.decode(f, t, j, dm)
        for p, m in errs:
            k = m.split(" at")[0]
            k = k if not k.startswith("bad opcode") else "bad opcode"
            kinds[k] += 1
            if len(examples) < 6:
                prev = max((q for q in ins if q < p), default=None)
                examples.append((i, hex(p), m, hex(prev) if prev is not None else None,
                                 hex(ins[prev][0]) if prev is not None else None))
    print(code, dict(kinds))
    for e in examples:
        print("    ", e)
