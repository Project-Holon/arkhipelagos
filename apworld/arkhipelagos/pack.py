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


_static = _data("data/static.json")
if _static is None:
    raise RuntimeError(
        "arkhipelagos: data/static.json is missing. Use a built arkhipelagos.apworld, or in a source "
        "checkout run: arkh-gen ap-static apworld/arkhipelagos/data/static.json")
STATIC: dict[str, Any] = json.loads(_static)


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
            f"this apworld has no arkhipelagos generator for this computer ({folder}). "
            f"Only these options need it: {TABLE_OPTIONS}. Turn them off, use a release "
            "that has a generator for this computer, or set ARKHIPELAGOS_GEN to an arkh-gen you built.")
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


TABLE_OPTIONS = "any randomize_* or replace_evo_methods modifier, a starter_pool, or species_sources other than the default"
DEFAULT_SOURCES = ["evolutions", "form_change", "static", "trainers"]


def needs_helper(opts: dict[str, Any]) -> bool:
    """True when the options change the game's tables (or the species the
    logic counts), so only the generator can make the pack."""
    if opts.get("starter_pool"):
        return True
    if sorted(opts.get("species_sources", DEFAULT_SOURCES)) != DEFAULT_SOURCES:
        return True
    return any(v for k, v in opts.items() if k.startswith("randomize_") or k == "replace_evo_methods")


def prebuilt_key(version: str, opts: dict[str, Any]) -> str:
    """The name of the prebuilt pack for options that keep the tables."""
    logic = "+".join(sorted(opts.get("modify_logic", []))) or "none"
    return f"{version}-{opts['goal']}-{opts['season_control']}-{logic}"


def prebuilt(version: str, opts: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]] | None:
    """A prebuilt pack and the configuration that made it, when the apworld
    has one for these options."""
    blob = _data(f"data/packs/{prebuilt_key(version, opts)}.json")
    if blob is None:
        return None
    v = json.loads(blob)
    return v["config"], v["pack"]


def _cannot_run(path: str, e: Exception) -> RuntimeError:
    return RuntimeError(
        f"could not run the arkhipelagos generator ({path}): {e}. "
        "On macOS, allow it in System Settings, Privacy and Security, or run "
        f"xattr -d com.apple.quarantine \"{path}\". On Linux, the cache folder must allow "
        "running programs. Or set ARKHIPELAGOS_GEN to an arkh-gen you built. "
        f"Only these options need it: {TABLE_OPTIONS}.")


def make_pack(config: dict[str, Any], player_index: int = 0) -> dict[str, Any]:
    """One player's pack from a generator configuration."""
    path = helper()
    try:
        r = subprocess.run(
            [path, "ap-pack", "-", str(player_index)],
            input=json.dumps(config).encode(),
            capture_output=True,
            timeout=300,
        )
    except (OSError, subprocess.SubprocessError) as e:
        raise _cannot_run(path, e) from e
    if r.returncode != 0:
        raise RuntimeError("arkhipelagos generator: " + r.stderr.decode(errors="replace").strip())
    pack = json.loads(r.stdout)
    if pack.get("data_hash") != STATIC.get("data_hash"):
        raise RuntimeError("arkhipelagos: the generator and the apworld's data differ")
    return pack
