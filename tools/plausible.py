"""For each command where BWAP's layout differs from the derived one: over all
uses in the 12 cartridges, how often does the next u16 under each layout
name a common command (one of the 120 most used)? A wrong length lands on
argument values instead, which are rarely common opcodes.

usage: python tools/plausible.py BWAP_script_commands.py
"""
import sys
from collections import Counter

import scriptdis as S

bw = S.bwap_table(sys.argv[1])
cache = list(S.corpus())
freq = Counter()
uses = []
t = S.Table("eu")
j, dm = S.ops_for("eu")
for code, files in cache:
    if S.fam_of(code) != "eu":
        continue
    for f in files:
        if S.kind_of(f) != "script":
            continue
        ins, _, _, _ = S.decode(f, t, j, dm)
        for p, (op, args, size) in ins.items():
            freq[op] += 1
            uses.append((f, p, op, size))
common = {op for op, _ in freq.most_common(120)}
res = {}
for f, p, op, size in uses:
    if op not in bw:
        continue
    alt = 2 + sum(S.SIZE[k] for k in bw[op]["kinds"])
    if alt == size:
        continue
    r = res.setdefault(op, [0, 0, 0])
    r[0] += 1
    nd = int.from_bytes(f[p + size:p + size + 2], "little")
    nb = int.from_bytes(f[p + alt:p + alt + 2], "little")
    r[1] += nd in common
    r[2] += nb in common
print("op     name                                        uses  derived-next-common  bwap-next-common  verdict")
for op, (n, d, b) in sorted(res.items()):
    verdict = "derived" if d > b else ("BWAP" if b > d else "tie")
    print(f"{op:#05x}  {t.cmds[op]['name'][:42]:42s} {n:5d} {d:12d} {b:18d}       {verdict}")
