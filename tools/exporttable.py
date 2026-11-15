"""Write data/script/commands.tsv, the command table the Rust crate reads.

usage: python tools/exporttable.py [BWAP script_commands.py]

One row per (family, opcode):
  family  eu (the ten European cartridges) or ja (the two Japanese ones)
  op      opcode, hex
  name    aletheia's handler name
  args    argument kinds, comma separated: u8 u16 u32 wv (work or constant)
          wp (work variable); a relative u32 is "code" or the data format
          ("movement", "warp")
  ends    1 when the command ends the flow (End, Return, Halt; Jump ends too)
  uses    how many times the 12 cartridges' scripts use it (reached code)
  check   "bwap-agrees", "bwap-differs", or "no-bwap" (European rows)
  source  aletheia source file of the handler
Empty table slots are left out.
"""
import importlib.util
import json
import os
import sys
from collections import Counter

import scriptdis as S

ROOT = S.ROOT
DATA_FORMAT = S.DATA_FORMAT


def main():
    bw = None
    if len(sys.argv) > 1:
        spec = importlib.util.spec_from_file_location("sc", sys.argv[1])
        m = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(m)
        bw = m.commands_table
    d = json.load(open(os.path.join(ROOT, "out", "script", "commands_derived.json")))
    uses = {"eu": Counter(), "ja": Counter()}
    for code, files in S.corpus():
        fam = S.fam_of(code)
        t = S.Table(fam)
        j, dm = S.ops_for(fam)
        for f in files:
            if S.kind_of(f) == "script":
                ins, _, _, _ = S.decode(f, t, j, dm)
                for op, _, _ in ins.values():
                    uses[fam][op] += 1
    rows = ["family\top\tname\targs\tends\tuses\tcheck\tsource"]
    for fam in ("eu", "ja"):
        for c in d[fam]:
            if c.get("empty"):
                continue
            kinds = []
            for a in c["args"]:
                if a.get("rel"):
                    kinds.append("code" if a.get("target") == "code" else DATA_FORMAT.get(c["name"], "data"))
                else:
                    kinds.append(a["kind"])
            check = "no-bwap"
            if bw is not None and fam == "eu" and c["op"] in bw:
                theirs = [int(x[0]) for x in bw[c["op"]][1:]]
                ours = [S.SIZE.get(a["kind"], 0) for a in c["args"]]
                check = "bwap-agrees" if ours == theirs else "bwap-differs"
            ends = 1 if c.get("ends") or c["name"] == "SCRIPT_WORK_CmdJump" else 0
            src = (c.get("candidates") or [{}])[0].get("file") or ""
            rows.append(f"{fam}\t{c['op']:#05x}\t{c['name']}\t{','.join(kinds)}\t{ends}\t{uses[fam][c['op']]}\t{check}\t{src}")
    out = os.path.join(ROOT, "data", "script", "commands.tsv")
    open(out, "w", encoding="utf-8", newline="\n").write("\n".join(rows) + "\n")
    n_eu = sum(1 for r in rows[1:] if r.startswith("eu"))
    differs_used = [r for r in rows[1:] if "bwap-differs" in r and int(r.split("\t")[5]) > 0]
    print(f"wrote {len(rows) - 1} rows ({n_eu} eu); BWAP differs on {sum('bwap-differs' in r for r in rows)} rows, "
          f"of which used in scripts: {len(differs_used)}")
    for r in differs_used:
        print("   ", r)


if __name__ == "__main__":
    main()
