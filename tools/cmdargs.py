"""Derive each field-script command's argument layout from aletheia's C source.

usage: python tools/cmdargs.py <aletheia src/game copy> [out dir]

For every handler in the command table (out/script/cmdtable.json), the
handler's body is read with the language switches applied (Japanese, or
the European build), and the reads from the script are listed in order:

  VM_ReadU16                    u16
  VM_GetU32                     u32
  SCRIPT_WORK_ReadWorkValue     wv   (u16: a work variable or a constant)
  SCRIPT_WORK_ReadWorkPointer   wp   (u16: a work variable)
  ReadByte / *vm->pc++          u8

Calls to other functions that take `vm` are followed. A read inside a
nested block (if, switch, loop) is marked conditional. A u32 that is added
to the program counter is a relative target. Whether a command ends the
flow (VM_End, VM_Return, an unconditional VM_Jump) is recorded too.

Output: data/script/commands.json, one table per language family
("eu" for the ten European cartridges, "ja" for the two Japanese ones).
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

READS = {
    "VM_ReadU16": "u16",
    "VM_GetU32": "u32",
    "SCRIPT_WORK_ReadWorkValue": "wv",
    "SCRIPT_WORK_ReadWorkPointer": "wp",
    "ReadByte": "u8",
}
SIZE = {"u8": 1, "u16": 2, "wv": 2, "wp": 2, "u32": 4}


def preprocess(text, japanese):
    out, stack = [], []          # stack of (active, taken_branch)
    for line in text.split("\n"):
        s = line.strip()
        m = re.match(r"#\s*(if|ifdef|ifndef|elif|else|endif)\b(.*)", s)
        if m:
            kw, cond = m.group(1), m.group(2).strip()
            if kw in ("if", "ifdef", "ifndef"):
                parent = all(a for a, _ in stack)
                v = evaluate(kw, cond, japanese)
                stack.append((parent and v, v))
            elif kw == "elif":
                a, taken = stack.pop()
                parent = all(x for x, _ in stack)
                v = (not taken) and evaluate("if", cond, japanese)
                stack.append((parent and v, taken or v))
            elif kw == "else":
                a, taken = stack.pop()
                parent = all(x for x, _ in stack)
                stack.append((parent and not taken, True))
            elif kw == "endif":
                stack.pop()
            out.append("")
            continue
        out.append(line if all(a for a, _ in stack) else "")
    return "\n".join(out)


def evaluate(kw, cond, japanese):
    if kw == "ifdef":
        return cond == "__MWERKS__"
    if kw == "ifndef":
        return cond != "__MWERKS__"
    c = cond.replace(" ", "")
    if c == "LANGUAGE==LANGUAGE_JAPANESE":
        return japanese
    if c == "LANGUAGE!=LANGUAGE_JAPANESE":
        return not japanese
    if c in ("PM_LANGUAGE_RUNTIME", "0"):
        return False
    return True


FUNC_RE = re.compile(r"^(?!\s)(?:static\s+)?(?:inline\s+)?[A-Za-z_][\w\s\*]*?\b([A-Za-z_]\w*)\s*\(([^;{}]*)\)\s*\{", re.M)


def functions(text):
    """name -> body text (between the outer braces)."""
    out = {}
    for m in FUNC_RE.finditer(text):
        name = m.group(1)
        if name in ("if", "while", "for", "switch", "return"):
            continue
        i = m.end()
        depth = 1
        j = i
        while j < len(text) and depth:
            c = text[j]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
            j += 1
        out.setdefault(name, (m.group(2), text[i:j - 1]))
    return out


CALL_RE = re.compile(r"\b([A-Za-z_]\w*)\s*\(")
CONTROL = ("if", "else", "for", "while", "switch", "case", "do")


def match_brace(t, i):
    """Index of the brace that closes the one at i."""
    d = 0
    for j in range(i, len(t)):
        if t[j] == "{":
            d += 1
        elif t[j] == "}":
            d -= 1
            if d == 0:
                return j
    return len(t) - 1


def strip_comments(t):
    t = re.sub(r"/\*.*?\*/", lambda m: " " * len(m.group(0)), t, flags=re.S)
    return re.sub(r"//[^\n]*", "", t)


class Analyzer:
    """funcs: name -> list of (file, params, body). A call is resolved in the
    caller's own file first (static helpers), then anywhere."""

    def __init__(self, funcs):
        self.funcs = funcs
        self.memo = {}

    def pick(self, name, file=None):
        cands = self.funcs.get(name, [])
        for c in cands:
            if c[0] == file:
                return c
        return cands[0] if cands else None

    def reads(self, name, stack=(), file=None):
        """List of (kind, conditional, rel) in order, plus flags."""
        key = (name, file)
        if key in self.memo:
            return self.memo[key]
        f = self.pick(name, file)
        if name in stack or f is None:
            return {"args": [], "ends": False, "jumps": False, "unknown": f is None}
        file, params, body = f
        body = strip_comments(body)
        vmname = "vm"
        for prm in params.split(","):
            if "ScriptVm" in prm or "VM *" in prm or "Vm *" in prm:
                mm = re.search(r"(\w+)\s*$", prm.strip())
                if mm:
                    vmname = mm.group(1)
                break
        res = {"args": [], "ends": False, "jumps": False, "unknown": False, "notes": []}
        # depth of each position: nesting inside control blocks
        depth = [0] * (len(body) + 1)
        d = 0
        for i, c in enumerate(body):
            if c == "{":
                d += 1
            elif c == "}":
                d -= 1
            depth[i] = d
        # a statement directly after if/for/while without braces is also conditional
        cond_spans = []
        for m in re.finditer(r"\b(if|for|while|switch)\s*\(", body):
            # find matching paren
            k, p = m.end(), 1
            while k < len(body) and p:
                p += {"(": 1, ")": -1}.get(body[k], 0)
                k += 1
            # condition text itself is evaluated always for `if`, so reads in it are not conditional
            rest = body[k:]
            s = len(rest) - len(rest.lstrip())
            if rest[s:s + 1] != "{":
                e = rest.find(";", s)
                cond_spans.append((k + s, k + (e if e >= 0 else s)))
        rel_vars = set(re.findall(r"(\w+)\s*=\s*\(?\s*(?:s32|int|u32)?\s*\)?\s*VM_GetU32\s*\(", body))
        pc_rel = bool(re.search(r"->pc\s*\+|pc\s*\+\s*\(|VM_Jump\s*\(|VM_Call\s*\(", body))
        events = [(m.start(), "call", m) for m in CALL_RE.finditer(body)]
        events += [(m.start(), "pc", m) for m in re.finditer(r"\b" + vmname + r"->pc\s*(\+\+|\+=\s*(\d+)|=\s*[\w>-]+\s*\+\s*(\d+))", body)]
        # if { A } else { B } where A and B read the same arguments: one read
        pairs = []
        for m in re.finditer(r"\bif\s*\(", body):
            k, p = m.end(), 1
            while k < len(body) and p:
                p += {"(": 1, ")": -1}.get(body[k], 0)
                k += 1
            a0 = body.find("{", k)
            if a0 < 0 or body[k:a0].strip():
                continue
            a1 = match_brace(body, a0)
            mm = re.match(r"\s*else\s*\{", body[a1 + 1:])
            if not mm:
                continue
            b0 = a1 + 1 + mm.end() - 1
            b1 = match_brace(body, b0)
            pairs.append(((a0, a1), (b0, b1), depth[m.start()]))
        # a function passed as a value (a wait callback, VM_SetWait) runs later
        # on the same vm and may read the remaining arguments
        events += [(m.start(), "cb", m) for m in re.finditer(r"(?<![\w.>])([A-Za-z_]\w*)\b(?!\s*\()", body)
                   if m.group(1) in self.funcs and m.group(1) != name
                   and "ScriptVm" in (self.pick(m.group(1), file) or ("", ""))[1]]
        events.sort(key=lambda x: x[0])
        for pos, kindev, m in events:
            if kindev == "cb":
                sub = self.reads(m.group(1), stack + (name,), file)
                sargs = sub["args"]
                # reads in alternative branches of the callback (switch cases,
                # one per save phase in WaitReportSave) happen once on a path
                if len(sargs) > 1 and all(a["cond"] for a in sargs) and len({a["kind"] for a in sargs}) == 1:
                    sargs = sargs[:1]
                    res["notes"].append(f"callback {m.group(1)}: same read in alternative branches, counted once")
                for a in sargs:
                    res["args"].append(dict(a, cond=True, later=True, pos=pos))
                if sargs:
                    res["notes"].append(f"reads in callback {m.group(1)}")
                continue
            if kindev == "pc":
                n = int(m.group(2) or m.group(3) or 1)
                conditional = depth[pos] > 0 or any(a <= pos < b for a, b in cond_spans)
                res["args"].append({"kind": {1: "u8", 2: "u16", 4: "u32"}.get(n, f"raw{n}"), "cond": conditional, "rel": False, "inline": True, "pos": pos})
                continue
            fn = m.group(1)
            conditional = depth[pos] > 0 or any(a <= pos < b for a, b in cond_spans)
            if fn in READS:
                kind = READS[fn]
                rel = False
                if kind == "u32" and pc_rel:
                    line = body[max(0, body.rfind("\n", 0, pos)):body.find("\n", pos)]
                    rel = ("pc" in line) or any(v in body and re.search(r"pc\s*\+\s*\(?\s*(?:s32)?\s*\)?\s*" + v, body) for v in rel_vars) \
                        or bool(re.search(r"VM_(Jump|Call)\s*\(", body))
                target = None
                if rel:
                    target = "code" if re.search(r"VM_(Jump|Call)\s*\(", body) else "data"
                res["args"].append({"kind": kind, "cond": conditional, "rel": rel, "target": target, "pos": pos})
            elif fn in ("VM_End",):
                res["ends"] = True
            elif fn in ("VM_Return",):
                res["ends"] = True
            elif fn in self.funcs and fn not in CONTROL and fn not in ("VM_ReadU16", "VM_GetU32"):
                # follow helpers that are passed the vm
                k, p = m.end(), 1
                while k < len(body) and p:
                    p += {"(": 1, ")": -1}.get(body[k], 0)
                    k += 1
                argtext = body[m.end():k - 1]
                if re.search(r"\b" + vmname + r"\b", argtext):
                    sub = self.reads(fn, stack + (name,), file)
                    for a in sub["args"]:
                        res["args"].append(dict(a, cond=a["cond"] or conditional, pos=pos))
                    res["ends"] |= sub["ends"] and not conditional
                    res["unknown"] |= sub.get("unknown", False)
        for (a0, a1), (b0, b1), d0 in pairs:
            ta = [x for x in res["args"] if a0 <= x.get("pos", -1) <= a1]
            tb = [x for x in res["args"] if b0 <= x.get("pos", -1) <= b1]
            if ta and [x["kind"] for x in ta] == [x["kind"] for x in tb]:
                drop = {id(x) for x in tb}
                res["args"] = [x for x in res["args"] if id(x) not in drop]
                if d0 == 0:
                    for x in ta:
                        x["cond"] = False
                res["notes"].append("same reads in both branches of an if/else: counted once")
        for x in res["args"]:
            x.pop("pos", None)
        res["jumps"] = bool(re.search(r"\bVM_Jump\s*\(", body))
        self.memo[key] = res
        return res


def main():
    src = sys.argv[1]
    out_dir = sys.argv[2] if len(sys.argv) > 2 else os.path.join(ROOT, "out", "script")
    table = json.load(open(os.path.join(ROOT, "out", "script", "cmdtable.json")))
    texts = {}
    for dp, _, fs in os.walk(src):
        for f in fs:
            if f.endswith(".c"):
                texts[os.path.join(dp, f)] = open(os.path.join(dp, f), encoding="utf-8", errors="replace").read()
    result = {}
    for fam, code, japanese in (("eu", "IRBO", False), ("ja", "IRBJ", True)):
        funcs = {}
        for path, t in texts.items():
            for k, (params, body) in functions(preprocess(t, japanese)).items():
                funcs.setdefault(k, []).append((path, params, body))
        an = Analyzer(funcs)
        cmds = []
        for e in table[code]["entries"]:
            name = e["name"]
            if e["addr"] == "0x00000000":
                cmds.append({"op": e["op"], "name": None, "empty": True})
                continue
            # Several functions can share a RAM address: overlays loaded in
            # turn. Every one that is a script command (its parameters take a
            # ScriptVm) is a candidate; which one runs depends on the overlay
            # the zone has loaded. All are kept; scripts decide (scriptcheck).
            cands = []
            for cand in e.get("candidates", []) or ([JA_ONLY[e["op"]]] if fam == "ja" and e["op"] in JA_ONLY else []):
                f = an.pick(cand)
                if f and "ScriptVm" in f[1]:
                    r = an.reads(cand)
                    cands.append({"name": cand, "file": os.path.relpath(f[0], src).replace("\\", "/"),
                                  "args": r["args"], "ends": r["ends"], "jumps": r["jumps"]})
            if not cands and name:
                # one candidate whose parameters do not spell ScriptVm (void *)
                r = an.reads(name)
                f = an.pick(name)
                cands.append({"name": name, "file": os.path.relpath(f[0], src).replace("\\", "/") if f else None,
                              "args": r["args"], "ends": r["ends"], "jumps": r["jumps"], "unknown": f is None,
                              "notes": r.get("notes", [])})
            layouts = {tuple(a["kind"] for a in c["args"]) for c in cands}
            first = cands[0] if cands else {"name": None, "args": [], "ends": False, "jumps": False}
            cmds.append({"op": e["op"], "name": first["name"], "args": first["args"], "ends": first["ends"],
                         "jumps": first["jumps"], "unknown": not cands or first.get("unknown", False),
                         "ambiguous": len(layouts) > 1, "candidates": cands})
        result[fam] = cmds
        unk = sum(1 for c in cmds if c.get("unknown"))
        condc = sum(1 for c in cmds if any(a["cond"] for a in c.get("args", [])))
        print(f"{fam}: {len(cmds)} commands, {unk} with no source found, {condc} with conditional reads")
    json.dump(result, open(os.path.join(out_dir, "commands_derived.json"), "w"), indent=1)


# Japanese-only handlers: the table entries aletheia's Japanese function maps
# cannot name, because their bodies differ from English. Named from the
# `#if LANGUAGE == LANGUAGE_JAPANESE` blocks of ov021/scrcmd_wordset_warp.c,
# by their place in the table between named neighbours.
JA_ONLY = {0x4D: "SCRCMD_BufferItemName", 0x55: "SCRCMD_BufferSpeciesName", 0x5E: "SCRCMD_BufferMapEntryNameA"}

if __name__ == "__main__":
    main()
