"""Check the in-game lines and their translations against each cartridge.

usage: python tools/checklines.py [--show]

For every line, in every language, on every cartridge of that language:
- every placeholder resolves from the cartridge's own text ({item:N} and
  the other term placeholders);
- a translation holds the same placeholders as the English line ({c:N},
  {buf:N} and the terms; order may change); a language file may list a
  line under "own_terms", with a reason, when its grammar needs a form of a
  name the cartridge does not hold; then only {c:N} and {buf:N} must match;
- every character is in the game's font;
- no line is wider than the widest line of the game's own text of the same
  kind, in that cartridge (messages: font 0 over the story text; item
  descriptions: font 2 over the item descriptions; menu entries: font 0
  over the item names); a {buf:N} counts as wide as the cartridge's widest
  item name, so a pass holds for any item or name in it;
- a message box shows two lines: at most two lines between box breaks
  (\\c clears the box, \\r scrolls);
- Japanese: the kana block holds no kanji.

Languages: en (IRBO, IRAO), fr (IRBF, IRAF), de, it, es, ja (IRBJ, IRAJ:
two texts per line, kana and kanji).
"""
import json
import os
import re
import sys
from difflib import SequenceMatcher

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import carts  # noqa: E402
import nftr  # noqa: E402
from textcodec import decode, narc_files  # noqa: E402

LANG_OF = {"O": "en", "F": "fr", "D": "de", "I": "it", "S": "es", "J": "ja"}
TOKEN = re.compile(r"\{(c|buf|item|species|place|name|badge):(\d+)\}|\\[nrc]")
TERM_KINDS = ("item", "species", "place", "name", "badge")


def strip_codes(units):
    out, k = [], 0
    while k < len(units):
        c = units[k]
        if c == 0xF000 and k + 2 < len(units):
            k += 3 + units[k + 2]
            continue
        if c == 0xFFFF:
            k += 1
            continue
        out.append(c)
        k += 1
    return out


class Cart:
    def __init__(self, code, en_counts):
        C = carts.load()
        self.code = code
        root = C[code]["data"]
        self.files = {arc: narc_files(os.path.join(root, "a", "0", "0", str(arc))) for arc in (2, 3)}
        fonts = narc_files(os.path.join(root, "a", "0", "2", "3"))
        self.font = {0: nftr.Font(fonts[0]), 2: nftr.Font(fonts[2])}
        self._dec = {}
        # file numbers of a/0/0/2 differ in Japanese: align by entry counts
        self.filemap = {2: self._align(2, en_counts[2]), 3: self._align(3, en_counts[3])}

    def decoded(self, arc, f):
        if (arc, f) not in self._dec:
            try:
                self._dec[(arc, f)] = decode(self.files[arc][f])
            except Exception:
                self._dec[(arc, f)] = None
        return self._dec[(arc, f)]

    def counts(self, arc):
        out = []
        for f in range(len(self.files[arc])):
            d = self.decoded(arc, f)
            out.append(len(d[0]) if d else -1)
        return out

    def _align(self, arc, en):
        mine = self.counts(arc)
        if mine == en:
            return {i: i for i in range(len(en))}
        m = {}
        for tag, i1, i2, j1, j2 in SequenceMatcher(None, en, mine, autojunk=False).get_opcodes():
            if tag == "equal":
                for k in range(i2 - i1):
                    m[i1 + k] = j1 + k
        return m

    def term(self, arc, f, entry, block):
        cf = self.filemap[arc].get(f)
        if cf is None:
            return None
        d = self.decoded(arc, cf)
        if not d or entry >= len(d[0]):
            return None
        b = d[min(block, len(d) - 1)]
        u = strip_codes(b[entry])
        return u or None

    def width(self, font, units):
        """width of each visual line"""
        widths, w, k = [], 0, 0
        while k < len(units):
            c = units[k]
            if c == 0xFFFE:
                widths.append(w)
                w = 0
            elif c == 0xF000 and k + 2 < len(units):
                cmd = units[k + 1]
                if cmd in (0xBE00, 0xBE01):
                    widths.append(w)
                    w = 0
                k += 3 + units[k + 2]
                continue
            elif c == 0xFFFF:
                pass
            else:
                a = self.font[font].advance(c)
                w += a if a is not None else 0
            k += 1
        widths.append(w)
        return widths


def game_limits(cart):
    """The widest line of the game's own text, per kind, and the most lines
    an item description has."""
    lim = {}
    best = 0
    for f in range(len(cart.files[3])):
        d = cart.decoded(3, f)
        if not d:
            continue
        for blk in d:
            for e in blk:
                best = max(best, max(cart.width(0, e)))
    lim["game"] = best
    desc = cart.decoded(2, cart.filemap[2][53])
    lim["item"] = max(max(cart.width(2, e)) for blk in desc for e in blk)
    lim["item_lines"] = max(len(cart.width(2, e)) for blk in desc for e in blk)
    names = cart.decoded(2, cart.filemap[2][54])
    lim["menu"] = max(max(cart.width(0, e)) for blk in names for e in blk)
    lim["item-name"] = lim["menu"]
    lim["buf"] = lim["menu"]
    return lim


def render(text, cart, block, terms):
    """our line syntax -> game units, or an error string"""
    out, pos = [], 0
    for m in TOKEN.finditer(text):
        out += [ord(ch) for ch in text[pos:m.start()]]
        pos = m.end()
        t = m.group(0)
        if t == "\\n":
            out.append(0xFFFE)
        elif t == "\\r":
            out += [0xF000, 0xBE00, 0]
        elif t == "\\c":
            out += [0xF000, 0xBE01, 0]
        else:
            kind, n = m.group(1), int(m.group(2))
            if kind == "c":
                out += [0xF000, 0xFF00, 1, n]
            elif kind == "buf":
                out += [0xF000, 0x0100, 1, n]
            else:
                arc, f = terms[kind]
                u = cart.term(arc, f, n, block)
                if u is None:
                    return f"{t} has no text in {cart.code}"
                out += u
    out += [ord(ch) for ch in text[pos:]]
    out.append(0xFFFF)
    return out


def tokens(text):
    return sorted(t.group(0) for t in TOKEN.finditer(text) if not t.group(0).startswith("\\"))


def codes_of(text):
    return sorted(t for t in tokens(text) if t.startswith(("{c:", "{buf:")))


def boxes_ok(text):
    """at most two lines between box breaks"""
    for seg in re.split(r"\\[cr]", text):
        if seg.count("\\n") + 1 > 2 and seg.strip("\\n"):
            # a segment that starts with \n after \r continues the box
            lines = [x for x in seg.split("\\n")]
            if len([x for x in lines if x]) > 2:
                return False
    return True


def main():
    show = "--show" in sys.argv
    en = json.load(open(os.path.join(ROOT, "lines", "en.json"), encoding="utf-8"))
    terms = {k: tuple(v) for k, v in en["terms"].items()}

    def norm(t):
        # a real line break in the JSON means the \n token
        if isinstance(t, dict):
            return {k: norm(v) for k, v in t.items()}
        return t.replace("\n", "\\n")

    trans = {"en": {l["id"]: norm(l["text"]) for l in en["lines"]}}
    # lines where a language writes a term itself, because its grammar needs
    # a form the cartridge does not hold; each entry gives the reason
    own_terms = {}
    for lang in ("fr", "de", "it", "es", "ja"):
        p = os.path.join(ROOT, "lines", f"{lang}.json")
        if os.path.exists(p):
            j = json.load(open(p, encoding="utf-8"))
            trans[lang] = {k: norm(v) for k, v in j["lines"].items()}
            own_terms[lang] = j.get("own_terms", {})
            for lid, why in own_terms[lang].items():
                if lid not in trans[lang] or not why.strip():
                    print(f"{lang}: own_terms entry {lid} has no line or no reason")
                    sys.exit(1)
    kinds = {l["id"]: l["kind"] for l in en["lines"]}
    ref = Cart("IRBO", {2: None, 3: None}) if False else None
    C0 = carts.load()
    base = Cart.__new__(Cart)
    # English counts for the file alignment
    base_files = {arc: narc_files(os.path.join(C0["IRBO"]["data"], "a", "0", "0", str(arc))) for arc in (2, 3)}
    en_counts = {}
    for arc in (2, 3):
        cs = []
        for d in base_files[arc]:
            try:
                cs.append(len(decode(d)[0]))
            except Exception:
                cs.append(-1)
        en_counts[arc] = cs
    errors = 0
    for code in carts.CODES:
        lang = LANG_OF[code[3]]
        if lang not in trans:
            print(f"{code}: no {lang} lines yet")
            continue
        cart = Cart(code, en_counts)
        lim = game_limits(cart)
        problems = []
        for lid, kind in kinds.items():
            src = trans["en"][lid]
            t = trans[lang].get(lid)
            if t is None:
                problems.append(f"{lid}: missing")
                continue
            variants = [("kana", t["kana"], 0), ("kanji", t["kanji"], 1)] if lang == "ja" else [("", t, 0)]
            for vname, text, block in variants:
                tag = f"{lid}{'/' + vname if vname else ''}"
                if lid in own_terms.get(lang, {}):
                    # the control codes must still match; terms may differ
                    if codes_of(text) != codes_of(src):
                        problems.append(f"{tag}: codes {codes_of(text)} differ from English {codes_of(src)}")
                elif tokens(text) != tokens(src):
                    problems.append(f"{tag}: placeholders {tokens(text)} differ from English {tokens(src)}")
                # 円 is the one kanji the game's own kana text uses (prices)
                if vname == "kana" and re.search(r"[㐀-䶿一-内冇-鿿]", text):
                    problems.append(f"{tag}: kanji in the kana block")
                units = render(text, cart, block, terms)
                if isinstance(units, str):
                    problems.append(f"{tag}: {units}")
                    continue
                font = 2 if kind == "item" else 0
                missing = sorted({chr(u) for u in strip_codes(units) if u != 0xFFFE and not cart.font[font].has(u)})
                if missing:
                    problems.append(f"{tag}: characters not in the font: {missing}")
                bufw = text.count("{buf:") * lim["buf"]
                widths = cart.width(font, units)
                widest = max(widths) + (bufw if bufw else 0)
                limit = lim[kind if kind in lim else "game"]
                if max(widths) > limit:
                    problems.append(f"{tag}: {max(widths)} px wide, the game's widest is {limit}")
                elif widest > limit and show:
                    problems.append(f"{tag}: note: with the longest name in a buffer, {widest} px (limit {limit})")
                if kind == "game" and not boxes_ok(text):
                    problems.append(f"{tag}: more than two lines in one box")
                if kind == "item" and len(widths) > lim["item_lines"]:
                    problems.append(f"{tag}: {len(widths)} lines, item descriptions have at most {lim['item_lines']}")
                if kind in ("menu", "item-name") and len(widths) > 1:
                    problems.append(f"{tag}: a menu entry has one line")
                if show and code in ("IRBO", "IRBF", "IRBJ") and not vname == "kanji":
                    print(f"   {code} {tag}: {''.join(chr(u) if u < 0xF000 else ('/' if u == 0xFFFE else '') for u in units)[:110]}")
        hard = [p for p in problems if "note:" not in p]
        errors += len(hard)
        print(f"{code} ({lang}): limits {lim}; problems {len(hard)}")
        for p in problems[:40]:
            print("    ", p)
    print("all lines pass" if errors == 0 else f"{errors} problems")
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
