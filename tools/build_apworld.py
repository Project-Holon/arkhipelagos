"""Build arkhipelagos.apworld, the Archipelago world.

    python tools/build_apworld.py --gen <arkh-gen> [--helper PLATFORM=PATH ...] --out <dir>

--gen is an arkh-gen for this machine: it writes the static file (every
item and location number) and the prebuilt packs (the original tables, for
every goal, season and logic option). Each --helper adds an arkh-gen the
world runs on the host for options that change the tables; PLATFORM is
windows-x86_64, linux-x86_64 or macos (the folder names pack.py looks for).

The zip holds one folder, arkhipelagos/, with archipelago.json carrying the
container versions Archipelago's "Build APWorlds" adds (version 7,
compatible_version 7 in Archipelago 0.6.9)."""

import argparse
import itertools
import json
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORLD = ROOT / "apworld" / "arkhipelagos"
CONTAINER_VERSION = 7
SKIP = {"__pycache__", "test", ".gitignore", "bin", "data"}

VERSIONS = ["black", "white"]
GOALS = ["ghetsis", "champion"]
SEASONS = ["vanilla", "changeable"]
LOGIC = ["require_flash", "require_dowsing"]
DEFAULT_SOURCES = ["evolutions", "form_change", "static", "trainers"]
PREBUILT_SEED = 1


def run(gen: str, args: list[str], stdin: bytes | None = None) -> bytes:
    r = subprocess.run([gen] + args, input=stdin, capture_output=True)
    if r.returncode != 0:
        sys.exit(f"{gen} {' '.join(args)}: {r.stderr.decode(errors='replace')}")
    return r.stdout


def prebuilt(gen: str, out: Path) -> int:
    """One pack per version, goal, season control and modify_logic set,
    with the original tables and the default species sources."""
    n = 0
    for version, goal, season, k in itertools.product(VERSIONS, GOALS, SEASONS, range(4)):
        logic = [LOGIC[i] for i in range(2) if k >> i & 1]
        opts = {"goal": goal, "season_control": season, "modify_logic": sorted(logic),
                "species_sources": DEFAULT_SOURCES, "starter_pool": []}
        config = {"schema": 1, "seed": PREBUILT_SEED, "options": {},
                  "players": [{"name": "player", "version": version, "options": opts}]}
        pack = json.loads(run(gen, ["ap-pack", "-", "0"], json.dumps(config).encode()))
        key = f"{version}-{goal}-{season}-{'+'.join(sorted(logic)) or 'none'}"
        (out / f"{key}.json").write_text(json.dumps({"config": config, "pack": pack}, separators=(",", ":")),
                                         encoding="utf-8")
        n += 1
    return n


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--gen", required=True)
    ap.add_argument("--helper", action="append", default=[])
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=out) as tmp:
        w = Path(tmp) / "arkhipelagos"
        shutil.copytree(WORLD, w, ignore=lambda d, names: [x for x in names if x in SKIP])
        (w / "data" / "packs").mkdir(parents=True)
        static = w / "data" / "static.json"
        run(a.gen, ["ap-static", str(static)])
        n = prebuilt(a.gen, w / "data" / "packs")
        for h in a.helper:
            plat, path = h.split("=", 1)
            name = "arkh-gen.exe" if plat.startswith("windows") else "arkh-gen"
            (w / "bin" / plat).mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, w / "bin" / plat / name)
        manifest = json.loads((WORLD / "archipelago.json").read_text(encoding="utf-8"))
        manifest["version"] = CONTAINER_VERSION
        manifest["compatible_version"] = CONTAINER_VERSION
        (w / "archipelago.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        target = out / "arkhipelagos.apworld"
        with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
            for f in sorted(w.rglob("*")):
                if f.is_file():
                    info = zipfile.ZipInfo(f.relative_to(Path(tmp)).as_posix(), (2026, 1, 1, 0, 0, 0))
                    info.compress_type = zipfile.ZIP_DEFLATED
                    # programs keep their run permission when unpacked
                    info.external_attr = (0o755 if f.parent.parent.name == "bin" else 0o644) << 16
                    z.writestr(info, f.read_bytes())
    print(f"{target}: {target.stat().st_size} bytes, {n} prebuilt packs, helpers: "
          f"{', '.join(h.split('=', 1)[0] for h in a.helper) or 'none'}")


if __name__ == "__main__":
    main()
