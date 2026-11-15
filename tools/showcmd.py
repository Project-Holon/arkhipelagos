"""Print script-command handler bodies as one language build sees them.

usage: python tools/showcmd.py <aletheia src/game copy> eu|ja NAME [NAME ...]
"""
import os
import sys

import cmdargs as A

src, fam = sys.argv[1], sys.argv[2]
funcs = {}
for dp, _, fs in os.walk(src):
    for f in fs:
        if f.endswith(".c"):
            p = os.path.join(dp, f)
            t = open(p, encoding="utf-8", errors="replace").read()
            for k, (pa, b) in A.functions(A.preprocess(t, fam == "ja")).items():
                funcs.setdefault(k, []).append((p, pa, b))
for name in sys.argv[3:]:
    for p, pa, b in funcs.get(name, []):
        print(f"==== {name}({pa})  [{os.path.relpath(p, src)}]")
        print(A.strip_comments(b).strip())
