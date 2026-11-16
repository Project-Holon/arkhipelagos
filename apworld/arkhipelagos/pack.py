"""The data this world reads: the static file (every item and location
number) and one pack per player, made by the arkhipelagos generator
(arkh-gen). Python makes no names, numbers, pools, tables or rules of its
own: the generator's are the only ones, so the two cannot drift apart."""

import hashlib
import json
import os
import pkgutil
import platform
import stat
import subprocess
from typing import Any

PACKAGE = __package__ or "arkhipelagos"


def _data(path: str) -> bytes | None:
    try:
        return pkgutil.get_data(PACKAGE, path)
    except (FileNotFoundError, OSError):
        return None


STATIC: dict[str, Any] = json.loads(_data("data/static.json") or b"{}")


def _platform() -> tuple[str, str]:
    """The helper's folder and file name for this computer."""
    system = platform.system().lower()
    machine = platform.machine().lower()
    arch = {"amd64": "x86_64", "x64": "x86_64", "arm64": "aarch64"}.get(machine, machine)
    if system == "windows":
        return f"windows-{arch}", "arkh-gen.exe"
    if system == "darwin":
        return "macos", "arkh-gen"
    return f"linux-{arch}", "arkh-gen"


def helper() -> str:
    """The path of the generator helper. ARKHIPELAGOS_GEN names one for
    development; otherwise the copy inside the apworld is written once to
    Archipelago's cache folder (a program cannot run from inside a zip)."""
    env = os.environ.get("ARKHIPELAGOS_GEN")
    if env:
        return env
    folder, name = _platform()
    blob = _data(f"bin/{folder}/{name}")
    if blob is None:
        raise RuntimeError(
            f"arkhipelagos: this apworld has no generator for {folder}; "
            "use a release that includes it")
    import Utils
    digest = hashlib.sha256(blob).hexdigest()[:16]
    path = Utils.cache_path("arkhipelagos", digest, name)
    if not os.path.exists(path):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        tmp = path + ".part"
        with open(tmp, "wb") as f:
            f.write(blob)
        os.chmod(tmp, os.stat(tmp).st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        os.replace(tmp, path)
    return path


def make_pack(config: dict[str, Any], player_index: int = 0) -> dict[str, Any]:
    """One player's pack from a generator configuration."""
    r = subprocess.run(
        [helper(), "ap-pack", "-", str(player_index)],
        input=json.dumps(config).encode(),
        capture_output=True,
        timeout=300,
    )
    if r.returncode != 0:
        raise RuntimeError("arkhipelagos generator: " + r.stderr.decode(errors="replace").strip())
    pack = json.loads(r.stdout)
    if pack.get("data_hash") != STATIC.get("data_hash"):
        raise RuntimeError("arkhipelagos: the generator and the apworld's data differ")
    return pack
