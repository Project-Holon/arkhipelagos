"""Where the clean data of the 12 cartridges is on this machine.

The paths are machine-local: they come from carts.local.json next to this
file (not committed; see carts.example.json). Each cartridge entry names:
  rom      the clean .nds file
  data     the extracted NitroFS data folder (optional; the ROM is used otherwise)
  ov       the folder of decompressed overlays (y9_NNNN.bin)
  arm9     the decompressed arm9 (arm9_dec.bin)
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
CODES = ("IRBO", "IRAO", "IRBF", "IRAF", "IRBD", "IRAD", "IRBI", "IRAI", "IRBS", "IRAS", "IRBJ", "IRAJ")


def load():
    path = os.path.join(HERE, "carts.local.json")
    with open(path, encoding="utf-8") as f:
        carts = json.load(f)
    missing = [c for c in CODES if c not in carts]
    if missing:
        raise SystemExit(f"carts.local.json lacks {missing}")
    return carts


def lang(code):
    return code[3]


def version(code):
    return "black" if code[2] == "B" else "white"
