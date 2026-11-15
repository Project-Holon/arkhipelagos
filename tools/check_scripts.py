"""Run `arkh-script check` on a/0/5/7 of all 12 cartridges.

usage: python tools/check_scripts.py [path to arkh-script binary]
"""
import os
import subprocess
import sys

import carts

ROOT = os.path.dirname(carts.HERE)
exe = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.environ.get("CARGO_TARGET_DIR", os.path.join(ROOT, "target")), "release", "arkh-script.exe")
C = carts.load()
bad = 0
for code in carts.CODES:
    path = os.path.join(C[code]["data"], "a", "0", "5", "7")
    fam = "ja" if code.endswith("J") else "eu"
    r = subprocess.run([exe, "check", fam, path], capture_output=True, text=True)
    out = (r.stdout + r.stderr).strip().replace(path, f"{code} a/0/5/7")
    print(out)
    bad += r.returncode != 0
print("all 12 cartridges pass" if bad == 0 else f"{bad} cartridges fail")
sys.exit(1 if bad else 0)
