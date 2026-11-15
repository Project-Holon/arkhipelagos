"""Message text of Pokemon Black and White (archives a/0/0/2 and a/0/0/3).

A text file: block count, entry count, then per block a table of (offset,
unit count, flags) and the entries. Each entry is 16-bit units, encrypted
with a key that runs backwards from the last unit (the end mark 0xFFFF);
the key of the previous unit is the next one rotated by 3.
Japanese files have two blocks (kana and kanji), the others one.
"""
import os
import struct


def narc_files(path):
    d = open(path, "rb").read()
    off = struct.unpack_from("<H", d, 0x0C)[0]
    btaf_size, count = struct.unpack_from("<IH", d, off + 4)
    spans = [struct.unpack_from("<II", d, off + 12 + 8 * i) for i in range(count)]
    off += btaf_size
    off += struct.unpack_from("<I", d, off + 4)[0]
    base = off + 8
    return [d[base + s:base + e] for s, e in spans]


def decode(data):
    """-> list of blocks, each a list of entries (lists of units)."""
    nblocks, nentries = struct.unpack_from("<HH", data, 0)
    offs = struct.unpack_from("<%dI" % nblocks, data, 12)
    blocks = []
    for bo in offs:
        entries = []
        for j in range(nentries):
            toff, n, _flags = struct.unpack_from("<IHH", data, bo + 4 + 8 * j)
            enc = struct.unpack_from("<%dH" % n, data, bo + toff)
            key = enc[-1] ^ 0xFFFF if n else 0
            dec = [0] * n
            for k in range(n - 1, -1, -1):
                dec[k] = enc[k] ^ key
                key = ((key >> 3) | (key << 13)) & 0xFFFF
            entries.append(dec)
        blocks.append(entries)
    return blocks


def read_texts(data_root):
    """{(archive 2|3, file, entry): [units per block]}"""
    out = {}
    for arc in (2, 3):
        files = narc_files(os.path.join(data_root, "a", "0", "0", str(arc)))
        for f, d in enumerate(files):
            try:
                blocks = decode(d)
            except Exception:
                continue
            for e in range(len(blocks[0]) if blocks else 0):
                out[(arc, f, e)] = [b[e] for b in blocks]
    return out


def plain(units):
    """Text without control codes, end marks removed."""
    s, k = [], 0
    while k < len(units):
        c = units[k]
        if c == 0xF000 and k + 2 < len(units):
            k += 3 + units[k + 2]
            continue
        if c in (0xFFFF, 0xFFFE):
            if c == 0xFFFE:
                s.append("\n")
            k += 1
            continue
        if 0x20 <= c <= 0xD7FF or 0xE000 <= c < 0xF000:
            s.append(chr(c))
        k += 1
    return "".join(s).strip()
