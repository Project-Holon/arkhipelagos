"""Walk two cartridges' decodings of one script member side by side and
report where the command streams first differ (opcode via the EN<->JA map).

usage: python tools/cmpdecode.py CART_A CART_B INDEX
"""
import json
import os
import sys

import scriptdis as S

a_code, b_code, idx = sys.argv[1], sys.argv[2], int(sys.argv[3])
C = S.carts.load()
t = json.load(open(os.path.join(S.ROOT, "out", "script", "cmdtable.json")))


def addr_map(code):
    return {e["op"]: e["en_addr"] for e in t[code]["entries"]}


def listing(code):
    files = S.narc_files(os.path.join(C[code]["data"], "a", "0", "5", "7"))
    fam = S.fam_of(code)
    tb = S.Table(fam)
    j, dm = S.ops_for(fam)
    ins, bl, errs, h = S.decode(files[idx], tb, j, dm)
    am = addr_map(code)
    return [(p, am.get(ins[p][0]), ins[p][0], [k for k, _ in ins[p][1]], tb.cmds[ins[p][0]]["name"]) for p in sorted(ins)], errs


la, ea = listing(a_code)
lb, eb = listing(b_code)
print(len(la), len(lb), "errors", len(ea), len(eb))
for (pa, aa, oa, ka, na), (pb, ab, ob, kb, nb) in zip(la, lb):
    if aa != ab or ka != kb:
        print("first difference:")
        print(f"  {a_code} {pa:#x} op {oa:#x} {na} {ka}")
        print(f"  {b_code} {pb:#x} op {ob:#x} {nb} {kb}")
        break
