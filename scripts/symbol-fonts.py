"""
Builds the symbol and emoji fonts of web/src/assets/fonts and prints their @font-face rules (app.css).

Run once by hand when they change, with fontTools + brotli (`pip install fonttools brotli`), on a
folder where these npm packages are unpacked (`npm pack <name>`, then `tar xzf … -C <dir>`):
  inter-ui-4.1.1                       → <dir>/inter-ui-4.1.1/package
  @fontsource/noto-sans-symbols-2@5.3.0 → <dir>/fontsource-noto-sans-symbols-2-5.3.0/package
  @fontsource/noto-sans-symbols@5.3.0   → <dir>/fontsource-noto-sans-symbols-5.3.0/package
  @fontsource/noto-sans-math@5.3.0      → <dir>/fontsource-noto-sans-math-5.3.0/package
  @expo-google-fonts/noto-sans-jp       → <dir>/notojp/package
  twemoji-colr-font@15.0.3              → <dir>/twemoji-colr-font-15.0.3/package

    python3 scripts/symbol-fonts.py <dir>
"""
import os
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

D = sys.argv[1]
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'web', 'src', 'assets', 'fonts') + '/'
SYMBOLS = set(range(0x2100, 0x2C00))
KATAKANA = set(range(0xFF61, 0xFFA0))
BOX = set(range(0x2500, 0x25A0))

# Emoji shown in colour (Emoji_Presentation=Yes in the BMP, plus ☀ which the Pet asks with FE0F);
# the other BMP symbols (⚠ ⏸ ▶ ♥…) stay text, in the text's colour.
BMP_EMOJI = [(0x231A, 0x231B), (0x23E9, 0x23EC), (0x23F0, 0x23F0), (0x23F3, 0x23F3), (0x25FD, 0x25FE), (0x2600, 0x2600), (0x2614, 0x2615), (0x2648, 0x2653), (0x267F, 0x267F), (0x2693, 0x2693), (0x26A1, 0x26A1), (0x26AA, 0x26AB), (0x26BD, 0x26BE), (0x26C4, 0x26C5), (0x26CE, 0x26CE), (0x26D4, 0x26D4), (0x26EA, 0x26EA), (0x26F2, 0x26F3), (0x26F5, 0x26F5), (0x26FA, 0x26FA), (0x26FD, 0x26FD), (0x2705, 0x2705), (0x270A, 0x270B), (0x2728, 0x2728), (0x274C, 0x274C), (0x274E, 0x274E), (0x2753, 0x2755), (0x2757, 0x2757), (0x2795, 0x2797), (0x27B0, 0x27B0), (0x27BF, 0x27BF), (0x2B1B, 0x2B1C), (0x2B50, 0x2B50), (0x2B55, 0x2B55)]
EMOJI = {c for a, b in BMP_EMOJI for c in range(a, b + 1)} | set(range(0x1F000, 0x1FB00)) | {0x200D, 0x20E3, 0xFE0F} | set(range(0xE0020, 0xE0080))


def codes(f):
    return {c for t in f['cmap'].tables if t.isUnicode() for c in t.cmap}


def cmap(p):
    return codes(TTFont(p))


def ranges(cps):
    cps = sorted(cps)
    out, start, prev = [], cps[0], cps[0]
    for c in cps[1:] + [None]:
        if c is not None and c == prev + 1:
            prev = c
            continue
        out.append(f'U+{start:04X}' if start == prev else f'U+{start:04X}-{prev:04X}')
        if c is not None:
            start = prev = c
    return ', '.join(out)


def cut(src, dst, cps):
    opts = subset.Options()
    opts.flavor = 'woff2'
    opts.layout_features = ['*']
    opts.notdef_outline = True
    s = subset.Subsetter(opts)
    f = TTFont(src)
    s.populate(unicodes=sorted(cps))
    s.subset(f)
    f.flavor = 'woff2'
    f.save(OUT + dst)
    return codes(f)


faces = []



def face(family, file, cps, weight, style='normal', display='swap'):
    faces.append(f"@font-face {{\n  font-family: '{family}';\n  src: url('./assets/fonts/{file}') format('woff2');\n  font-weight: {weight};\n  font-style: {style};\n  font-display: {display};\n  unicode-range: {ranges(cps)};\n}}")


# Inter's own symbols (arrows, ⌘ ⌥ ⇧, ✓, ★…), which the latin subsets leave out.
inter_full = D + '/inter-ui-4.1.1/package/variable/'
latin = cmap(OUT + 'inter-latin-wght-normal.woff2') | cmap(OUT + 'inter-latin-ext-wght-normal.woff2')
inter_sym = (cmap(inter_full + 'InterVariable.woff2') & SYMBOLS) - latin - EMOJI
for style, src in (('normal', 'InterVariable.woff2'), ('italic', 'InterVariable-Italic.woff2')):
    got = cut(inter_full + src, f'inter-symbols-wght-{style}.woff2', inter_sym)
    face('Inter', f'inter-symbols-wght-{style}.woff2', got & inter_sym, '100 900', style)

# Box drawing and blocks for the code font: Recursive's own (from the terminals' Nerd Font).
got = cut(OUT + 'RecMonoCasualNerdFontMono-Regular.woff2', 'recursive-mono-box.woff2', BOX)
face('Recursive Mono', 'recursive-mono-box.woff2', got & BOX, '300 1000')

# The other symbols, one source per code point (the first that has it).
taken = set(inter_sym) | EMOJI
for name, src in (
    ('noto-symbols-2', D + '/fontsource-noto-sans-symbols-2-5.3.0/package/files/noto-sans-symbols-2-symbols-400-normal.woff2'),
    ('noto-symbols-2-math', D + '/fontsource-noto-sans-symbols-2-5.3.0/package/files/noto-sans-symbols-2-math-400-normal.woff2'),
    ('noto-symbols', D + '/fontsource-noto-sans-symbols-5.3.0/package/files/noto-sans-symbols-symbols-400-normal.woff2'),
    ('noto-math', D + '/fontsource-noto-sans-math-5.3.0/package/files/noto-sans-math-latin-400-normal.woff2'),
):
    want = (cmap(src) & SYMBOLS) - taken - BOX
    if not want:
        continue
    got = cut(src, f'prism-symbols-{name}.woff2', want)
    taken |= got & want
    face('Prism Symbols', f'prism-symbols-{name}.woff2', got & want, '400')
got = cut(D + '/notojp/package/400Regular/NotoSansJP_400Regular.ttf', 'prism-symbols-katakana.woff2', KATAKANA)
face('Prism Symbols', 'prism-symbols-katakana.woff2', got & KATAKANA, '400')

# Twemoji, whole: also the emoji of tickets, notes and agents' answers. Its variation selector and
# its joiner take a whole emoji's width: none, as they are invisible (☀️ left a blank after it).
tw = TTFont(D + '/twemoji-colr-font-15.0.3/package/twemoji.woff2')
for cp in (0xFE0F, 0x200D):
    g = tw.getBestCmap()[cp]
    tw['hmtx'][g] = (0, tw['hmtx'][g][1])
tw.save(OUT + 'twemoji.woff2')
face('Twemoji', 'twemoji.woff2', EMOJI, '100 1000')

print('\n'.join(faces))
