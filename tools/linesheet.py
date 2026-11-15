"""Render every line in every language with the cartridge's own names filled
in, for the review pass. Writes out/lines-sheet.txt.

usage: python tools/linesheet.py
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import checklines as CL  # noqa: E402
import carts  # noqa: E402
from textcodec import decode, narc_files  # noqa: E402

CARTS = {"en": "IRBO", "fr": "IRBF", "de": "IRBD", "it": "IRBI", "es": "IRBS", "ja": "IRBJ"}


def show(units):
    out, k = [], 0
    while k < len(units):
        c = units[k]
        if c == 0xF000 and k + 2 < len(units):
            cmd = units[k + 1]
            out.append({0xBE01: " |c| ", 0xBE00: " |r| ", 0x0100: "<buf%d>" % units[k + 3] if units[k + 2] else "<buf>"}.get(cmd, ""))
            k += 3 + units[k + 2]
            continue
        if c == 0xFFFE:
            out.append(" / ")
        elif c != 0xFFFF:
            out.append(chr(c))
        k += 1
    return "".join(out)


def main():
    en = json.load(open(os.path.join(ROOT, "lines", "en.json"), encoding="utf-8"))
    terms = {k: tuple(v) for k, v in en["terms"].items()}
    C0 = carts.load()
    en_counts = {}
    for arc in (2, 3):
        cs = []
        for d in narc_files(os.path.join(C0["IRBO"]["data"], "a", "0", "0", str(arc))):
            try:
                cs.append(len(decode(d)[0]))
            except Exception:
                cs.append(-1)
        en_counts[arc] = cs
    texts = {"en": {l["id"]: l["text"] for l in en["lines"]}}
    for lang in ("fr", "de", "it", "es", "ja"):
        texts[lang] = json.load(open(os.path.join(ROOT, "lines", f"{lang}.json"), encoding="utf-8"))["lines"]
    carts_ = {lang: CL.Cart(code, en_counts) for lang, code in CARTS.items()}
    out = []
    for l in en["lines"]:
        lid = l["id"]
        out.append(f"== {lid} ({l['kind']})")
        for lang in ("en", "fr", "de", "it", "es", "ja"):
            t = texts[lang][lid]
            vs = [("kana", t["kana"], 0), ("kanji", t["kanji"], 1)] if lang == "ja" else [("", t, 0)]
            for vn, text, block in vs:
                u = CL.render(text.replace("\n", "\\n"), carts_[lang], block, terms)
                out.append(f"  {lang}{'-' + vn if vn else ''}: {show(u) if not isinstance(u, str) else u}")
    p = os.path.join(ROOT, "out", "lines-sheet.txt")
    open(p, "w", encoding="utf-8").write("\n".join(out) + "\n")
    print("wrote", p)


if __name__ == "__main__":
    main()
