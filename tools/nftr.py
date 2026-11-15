"""The game's NFTR fonts (archive a/0/2/3): which characters a font has, and
how wide each is. Member 0 is 12x15 (messages, labels, names), member 1
10x10 (small labels), member 2 11x13 (descriptions).

Layout (as eidos's engine/ui/assets.c reads it): "RTFN" header, then FNIF
(at +8, +12, +16 of its body: the offsets of the CGLP, CWDH and first CMAP
bodies), PLGC (cell width and height at +0 and +1, record size at +2;
records from +8: three metric bytes, left, drawn width, advance, then the
bitmap), HDWC (not used: the metric bytes in the records are what the game
draws with), PAMC (a chain of maps: method 0 a range, 1 a table, 2 pairs;
an earlier map wins).
"""
import struct


class Font:
    def __init__(self, data):
        if data[:4] != b"RTFN":
            raise ValueError("not an NFTR font")
        p = struct.unpack_from("<H", data, 0x0C)[0]
        if data[p:p + 4] != b"FNIF":
            raise ValueError("FNIF missing")
        body = p + 8
        self.line_height = data[body + 1]
        cglp, _cwdh, cmap = struct.unpack_from("<III", data, body + 8)
        self.cell_w, self.cell_h = data[cglp], data[cglp + 1]
        rec = struct.unpack_from("<H", data, cglp + 2)[0]
        self._data, self._rec, self._glyphs = data, rec, cglp + 8
        self.index = {}
        off = cmap
        # the last map's link points just past the end of the file
        while off and off + 12 <= len(data):
            first, last, method = struct.unpack_from("<HHH", data, off)
            nxt = struct.unpack_from("<I", data, off + 8)[0]
            b = off + 12
            if method == 0:
                start = struct.unpack_from("<H", data, b)[0]
                for c in range(first, last + 1):
                    self.index.setdefault(c, start + (c - first))
            elif method == 1:
                for k, c in enumerate(range(first, last + 1)):
                    g = struct.unpack_from("<H", data, b + 2 * k)[0]
                    if g != 0xFFFF:
                        self.index.setdefault(c, g)
            elif method == 2:
                n = struct.unpack_from("<H", data, b)[0]
                for k in range(n):
                    c, g = struct.unpack_from("<HH", data, b + 2 + 4 * k)
                    self.index.setdefault(c, g)
            off = nxt

    def has(self, code):
        return code in self.index

    def advance(self, code):
        g = self.index.get(code)
        if g is None:
            return None
        return self._data[self._glyphs + g * self._rec + 2]
