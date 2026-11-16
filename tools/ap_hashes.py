"""Hash what the apworld's generator helper gives: the static file and the
pack of every player of every test configuration. CI runs this on Windows,
Linux and macOS and checks that the three lists are the same.

    python tools/ap_hashes.py <arkh-gen>"""

import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> None:
    gen = sys.argv[1]
    with tempfile.TemporaryDirectory() as d:
        static = Path(d) / "static.json"
        subprocess.run([gen, "ap-static", str(static)], check=True)
        print(hashlib.sha256(static.read_bytes()).hexdigest(), "static")
    for cfg in sorted((ROOT / "tests" / "configs").glob("*.json")):
        players = len(json.loads(cfg.read_text(encoding="utf-8"))["players"])
        for i in range(players):
            out = subprocess.run([gen, "ap-pack", str(cfg), str(i)], check=True, capture_output=True).stdout
            print(hashlib.sha256(out).hexdigest(), f"{cfg.name} {i}")


if __name__ == "__main__":
    main()
