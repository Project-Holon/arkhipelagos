"""List a wasm module's imports and exports, and check the imports against
eidos's API header.

usage: python tools/wasm_imports.py <module.wasm> [eidos_wasm.h]

With a header, every import must be module "eidos" and a name the header
declares (EIDOS_IMPORT("name")), as eidos's loader requires; the exports
must hold eidos_mod_start, eidos_mod_stop, the memory and the table.
"""
import re
import sys


def leb(d, p):
    r = s = 0
    while True:
        b = d[p]
        p += 1
        r |= (b & 0x7F) << s
        s += 7
        if b < 0x80:
            return r, p


def name(d, p):
    n, p = leb(d, p)
    return d[p:p + n].decode(), p + n


def sections(d):
    assert d[:4] == b"\0asm", "not a wasm module"
    p = 8
    while p < len(d):
        sid = d[p]
        size, q = leb(d, p + 1)
        yield sid, d[q:q + size]
        p = q + size


def imports_exports(d):
    imps, exps = [], []
    for sid, s in sections(d):
        if sid == 2:
            n, p = leb(s, 0)
            for _ in range(n):
                mod, p = name(s, p)
                nm, p = name(s, p)
                kind = s[p]
                p += 1
                if kind == 0:
                    _, p = leb(s, p)
                elif kind == 1:
                    p += 1
                    flags, p = leb(s, p)
                    _, p = leb(s, p)
                    if flags & 1:
                        _, p = leb(s, p)
                elif kind == 2:
                    flags, p = leb(s, p)
                    _, p = leb(s, p)
                    if flags & 1:
                        _, p = leb(s, p)
                elif kind == 3:
                    p += 2
                imps.append((mod, nm, kind))
        elif sid == 7:
            n, p = leb(s, 0)
            for _ in range(n):
                nm, p = name(s, p)
                kind = s[p]
                _, p = leb(s, p + 1)
                exps.append((nm, kind))
    return imps, exps


def main():
    d = open(sys.argv[1], "rb").read()
    imps, exps = imports_exports(d)
    for m, n, k in imps:
        print("import", m, n, k)
    print("exports:", ", ".join(n for n, _ in exps))
    if len(sys.argv) > 2:
        declared = set(re.findall(r'EIDOS_IMPORT\("(\w+)"\)', open(sys.argv[2], encoding="utf-8").read()))
        bad = [(m, n) for m, n, k in imps if m != "eidos" or n not in declared or k != 0]
        names = {n for n, _ in exps}
        missing = [x for x in ("eidos_mod_start", "eidos_mod_stop", "memory") if x not in names]
        if not any(k == 1 for _, k in exps):
            missing.append("a table")
        if bad or missing:
            raise SystemExit(f"not loadable: imports {bad}, missing exports {missing}")
        print(f"{len(imps)} imports, all declared by the header")


if __name__ == "__main__":
    main()
