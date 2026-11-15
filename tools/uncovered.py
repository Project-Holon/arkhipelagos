"""What the bytes no decoding reaches are, and what the unclassified members are."""
from collections import Counter

import scriptdis as S

kinds = Counter()
unknown = []
for code, files in S.corpus():
    fam = S.fam_of(code)
    t = S.Table(fam)
    j, dm = S.ops_for(fam)
    for i, f in enumerate(files):
        k = S.kind_of(f)
        if k == "unknown":
            unknown.append((code, i, len(f), f[:12].hex()))
            continue
        if k != "script":
            continue
        ins, blocks, errs, hend = S.decode(f, t, j, dm)
        cov = bytearray(len(f))
        cov[:hend] = b"\1" * hend
        for p, (op, a, n) in ins.items():
            cov[p:p + n] = b"\1" * n
        for p, n in blocks.items():
            cov[p:p + n] = b"\1" * n
        p = 0
        while p < len(f):
            if cov[p]:
                p += 1
                continue
            q = p
            while q < len(f) and not cov[q]:
                q += 1
            gap = f[p:q]
            if q == len(f) and len(gap) <= 3 and set(gap) <= {0}:
                kinds["end padding"] += len(gap)
            elif q == len(f):
                kinds["unreached tail"] += len(gap)
            elif set(gap) <= {0}:
                kinds["zero gap"] += len(gap)
            else:
                # does it decode as commands from its start?
                sub_ins = {}
                pc, ok = p, True
                while pc < q:
                    op = int.from_bytes(f[pc:pc + 2], "little")
                    c = t.cmds.get(op)
                    if c is None:
                        ok = False
                        break
                    pc += 2 + sum(S.SIZE[x] for x in c["kinds"])
                kinds["unreached code" if ok and pc == q else "unreached other"] += len(gap)
            p = q
print(dict(kinds))
print("unclassified members:", len(unknown))
for u in unknown[:12]:
    print("   ", u)
