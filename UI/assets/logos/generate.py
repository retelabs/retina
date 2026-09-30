# Generates the Retina mark: a spectral iris. A radial equaliser read three ways
# (an iris, a face-on galaxy, a live metrics dial), a star for a pupil, and one
# tilted orbit carrying a moon. Run from this directory:
#   python3 generate.py
# PNG exports: magick -background none retina-mark-dark.svg -resize 1024x1024 retina-mark-dark-1024.png
import math

NAVY = "#0B1026"
GOLD = "#F2C14E"
VIOLET = "#8B7CF8"
IVORY = "#F6F3EA"


def f(v):
    return f"{v:.2f}"


def amplitude(a):
    """Bar length profile: a few slow harmonics, so neighbours stay close
    (it reads as one spectrum, not noise) while the ring stays uneven."""
    return (0.55 + 0.25 * math.sin(3 * a) + 0.15 * math.sin(7 * a + 1.3)
            + 0.08 * math.sin(13 * a + 0.4))


def mark(bar, star, orbit, bg, with_bg=True, small=False):
    g = []
    if with_bg:
        h = 84 if small else 120
        g.append(f'<rect x="{-h}" y="{-h}" width="{2 * h}" height="{2 * h}" fill="{bg}"/>')
    n = 12 if small else 44
    width = 11 if small else 3.4
    r0 = 34 if small else 30
    angles = [2 * math.pi * i / n - math.pi / 2 for i in range(n)]
    amps = [amplitude(a) for a in angles]
    # Gold marks the spectrum's three highest peaks (local maxima), so the
    # accents read as signal, not as decoration.
    peaks = [i for i in range(n) if amps[i] >= amps[i - 1] and amps[i] >= amps[(i + 1) % n]]
    highlight = set() if small else set(sorted(peaks, key=lambda i: -amps[i])[:3])
    tilt = -24
    # The half of the orbit behind the iris is drawn first, the front half last.
    rx, ry = 94, 30
    back = f'<path d="M{-rx},0 A{rx},{ry} 0 0 1 {rx},0" fill="none" stroke="{orbit}" stroke-width="{3 if small else 1.6}" transform="rotate({tilt})"/>'
    front = f'<path d="M{rx},0 A{rx},{ry} 0 0 1 {-rx},0" fill="none" stroke="{orbit}" stroke-width="{3 if small else 1.6}" transform="rotate({tilt})"/>'
    if not small:
        g.append(back)
    for i, (a, amp) in enumerate(zip(angles, amps)):
        r1 = r0 + (12 if small else 8) + (30 if small else 30) * amp
        x0, y0 = r0 * math.cos(a), r0 * math.sin(a)
        x1, y1 = r1 * math.cos(a), r1 * math.sin(a)
        c = star if i in highlight else bar
        g.append(f'<line x1="{f(x0)}" y1="{f(y0)}" x2="{f(x1)}" y2="{f(y1)}" stroke="{c}" stroke-width="{width}" stroke-linecap="round"/>')
    if not small:
        g.append(front)
        t = 0.55
        mx, my = rx * math.cos(t), ry * math.sin(t)
        g.append(f'<circle cx="{f(mx)}" cy="{f(my)}" r="5" fill="{star}" transform="rotate({tilt})"/>')
    # Pupil: a star with a dark core, so it also reads as an eye.
    g.append(f'<circle cx="0" cy="0" r="{20 if small else 15}" fill="{star}"/>')
    g.append(f'<circle cx="0" cy="0" r="{8 if small else 6}" fill="{bg}"/>')
    return "".join(g)


def svg(inner, size=1024, half=120):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" '
            f'viewBox="{-half} {-half} {2 * half} {2 * half}">{inner}</svg>')


DARK = dict(bar=VIOLET, star=GOLD, orbit="#5A6090", bg=NAVY)
LIGHT = dict(bar="#4B3FC4", star="#C98A00", orbit="#8A8FB5", bg=IVORY)

files = {
    "retina-mark-dark.svg": mark(**DARK),
    "retina-mark-light.svg": mark(**LIGHT),
    "retina-mark-transparent.svg": mark(**DARK, with_bg=False),
    "retina-mark-small-dark.svg": mark(**DARK, small=True),
    "retina-mark-small-light.svg": mark(**LIGHT, small=True),
}
for name, inner in files.items():
    with open(name, "w") as out:
        out.write(svg(inner, half=84 if "small" in name else 120))
