"""For disputed commands, show real uses: the bytes after the opcode and what
follows under the derived layout and under BWAP's.

usage: python tools/evidence.py BWAP_script_commands.py [op ...]
"""
import os
import sys

import scriptdis as S

bw = S.bwap_table(sys.argv[1])
ops = [int(x, 16) for x in sys.argv[2:]]
code, files = next(iter(S.corpus()))      # English Black
t = S.Table("eu")
j, dm = S.ops_for("eu")
shown = {op: 0 for op in ops}
for i, f in enumerate(files):
    if S.kind_of(f) != "script":
        continue
    ins, _, _, _ = S.decode(f, t, j, dm)
    for p, (op, args, size) in sorted(ins.items()):
        if op in shown and shown[op] < 3:
            shown[op] += 1
            nxt = p + size
            alt = p + 2 + sum(S.SIZE[k] for k in bw[op]["kinds"])
            def name_at(q):
                o = int.from_bytes(f[q:q + 2], "little")
                c = t.cmds.get(o)
                return f"{o:#06x} {c['name'] if c else '<bad>'}"
            print(f"{op:#05x} {t.cmds[op]['name']} in member {i} at {p:#x}: bytes after opcode {f[p + 2:p + 12].hex(' ')}")
            print(f"      derived: args {[hex(v) for _, v in args]}, next {name_at(nxt)}  (reached: {nxt in ins})")
            print(f"      BWAP:    next {name_at(alt)}  (reached by derived decode: {alt in ins})")
