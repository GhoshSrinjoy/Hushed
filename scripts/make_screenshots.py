"""Render hushed's real terminal output as SVG screenshots for the README and docs.

    python scripts/make_screenshots.py

Needs hushed built in place (`maturin develop`) or installed. Writes docs/images/*.svg.
"""
import html
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "docs", "images")
SHOTS = {
    "run.svg": ["run", "examples/pipeline.py"],
    "analyze.svg": ["analyze", "examples/pipeline.py"],
}

FONT = "ui-monospace, 'Cascadia Mono', 'SF Mono', Menlo, Consolas, 'DejaVu Sans Mono', monospace"
SIZE, CHAR, LINE, PAD, TOP = 14, 8.43, 19, 20, 44
BACKGROUND, TEXT, PROMPT = "#1e1f29", "#d8dae5", "#7f849c"
COLORS = {"31": "#ff6b81", "32": "#7bd88f", "33": "#f5c66b", "36": "#6fc3df"}
ANSI = re.compile(r"\x1b\[([0-9;]*)m")


def capture(args):
    env = {**os.environ, "FORCE_COLOR": "1", "PYTHONIOENCODING": "utf-8", "PYTHONUNBUFFERED": "1"}
    env.pop("NO_COLOR", None)
    proc = subprocess.run([sys.executable, "-m", "hushed", *args], cwd=ROOT, env=env,
                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding="utf-8")
    return proc.stdout.strip("\n").splitlines()


def segments(line):
    """Split one line of ANSI-coloured text into (text, colour, bold, dim) runs."""
    runs, colour, bold, dim, pos = [], None, False, False, 0
    for m in ANSI.finditer(line):
        if m.start() > pos:
            runs.append((line[pos:m.start()], colour, bold, dim))
        for code in (m.group(1) or "0").split(";"):
            if code == "0":
                colour, bold, dim = None, False, False
            elif code == "1":
                bold = True
            elif code == "2":
                dim = True
            elif code in COLORS:
                colour = COLORS[code]
        pos = m.end()
    if pos < len(line):
        runs.append((line[pos:], colour, bold, dim))
    return runs


def tspan(text, column, y, colour=None, bold=False, dim=False):
    attrs = [f'x="{PAD + column * CHAR:.1f}"', f'y="{y}"']
    if len(text) > 1:   # pin each run to its columns, whatever the viewer's font
        attrs += [f'textLength="{len(text) * CHAR:.1f}"', 'lengthAdjust="spacing"']
    if colour:
        attrs.append(f'fill="{colour}"')
    if bold:
        attrs.append('font-weight="bold"')
    if dim:
        attrs.append('opacity="0.6"')
    return f"<tspan {' '.join(attrs)}>{html.escape(text, quote=False)}</tspan>"


def render(command, lines):
    rows = [[("$ ", PROMPT, False, False), (command, TEXT, True, False)]] + [segments(line) for line in lines]
    width = PAD * 2 + CHAR * max(sum(len(t) for t, *_ in row) for row in rows)
    height = TOP + LINE * len(rows) + PAD
    spans = []
    for i, row in enumerate(rows):
        y, column = TOP + LINE * i + SIZE, 0
        for text, colour, bold, dim in row:
            if text.strip():
                spans.append(tspan(text, column, y, colour, bold, dim))
            column += len(text)
    dots = "".join(f'<circle cx="{20 + 20 * i}" cy="20" r="6" fill="{c}"/>'
                   for i, c in enumerate(["#ff5f57", "#febc2e", "#28c840"]))
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0f}" height="{height}" '
            f'viewBox="0 0 {width:.0f} {height}" role="img" aria-label="{html.escape(command)}">\n'
            f'<rect width="100%" height="100%" rx="10" fill="{BACKGROUND}"/>\n{dots}\n'
            f'<text font-family="{FONT}" font-size="{SIZE}" fill="{TEXT}" xml:space="preserve">\n'
            + "\n".join(spans) + "\n</text>\n</svg>\n")


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, args in SHOTS.items():
        svg = render("hushed " + " ".join(args), capture(args))
        with open(os.path.join(OUT, name), "w", encoding="utf-8", newline="\n") as f:
            f.write(svg)
        print("wrote", os.path.relpath(os.path.join(OUT, name), ROOT))


if __name__ == "__main__":
    main()
