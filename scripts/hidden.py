#!/usr/bin/env python3
"""Prints the table of core/src/core/hidden.tbl from a UnicodeData.txt (SAFE-10).

    python3 -I scripts/hidden.py UnicodeData.txt > core/src/core/hidden.tbl

The chars to show are those of general category Cf, Cn (a code point UnicodeData.txt does not list),
the C1 controls, and the invisible chars SAFE-10 names that have other categories. Private use (Co)
is not among them: it draws a glyph, an icon of a Nerd Font or a box. The
output is the sorted bounds of the runs of such chars in hex, the first char of a run and then the
first char after it, in lines of 96 chars at most, for `hidden` in core/src/core/hidden.rs.
"""
import sys

cat, first = {}, 0
for line in open(sys.argv[1], encoding="utf-8"):
    f = line.split(";")
    cp, name, gc = int(f[0], 16), f[1], f[2]
    if name.endswith(", First>"):
        first = cp
    elif name.endswith(", Last>"):
        cat.update({c: gc for c in range(first, cp + 1)})
    else:
        cat[cp] = gc

extra = [0x34F, 0x115F, 0x1160, 0x17B4, 0x17B5, 0x2028, 0x2029, 0x3164, 0xFFA0]
extra += [*range(0x80, 0xA0), *range(0x180B, 0x1810), *range(0xFE00, 0xFE10), *range(0xE0100, 0xE01F0)]
hid = {c for c in range(0x110000) if cat.get(c, "Cn") in ("Cf", "Cn")} | set(extra)
hid -= set(range(0xD800, 0xE000))  # surrogates are not chars

bounds = []
for c in sorted(hid):
    if bounds and bounds[-1] == c:
        bounds[-1] = c + 1
    else:
        bounds += [c, c + 1]

line = ""
for b in bounds:
    word = f"{b:X}"
    if len(line) + len(word) + 1 > 96:
        print(line.rstrip())
        line = ""
    line += word + " "
print(line.rstrip())
print(f"{len(bounds) // 2} runs, {sum(b - a for a, b in zip(bounds[::2], bounds[1::2]))} chars", file=sys.stderr)
