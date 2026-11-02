"""Renders benchmarks/results.json into benchmarks/benchmark.svg for the README.

Usage: python3 benchmarks/make_chart.py
"""

import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
RESULTS = json.loads((HERE / "results.json").read_text())
OUT = HERE / "benchmark.svg"

TOOLS = [
    ("py-doc", "py-doc"),
    ("pydoc", "pydoc (stdlib)"),
    ("pdoc3", "pdoc3"),
    ("pdoc", "pdoc"),
    ("sphinx", "Sphinx + autodoc"),
]

WIDTH = 820
LABEL_W = 150  # tool-name column
VALUE_W = 150  # room right of the longest bar for its value label
BAR_H = 18
ROW_GAP = 8
PANEL_TOP = 46  # space for panel title + subtitle
PANEL_GAP = 26
LEFT = 16


def fmt_time(s: float) -> str:
    if s < 1:
        return f"{s * 1000:.0f} ms"
    if s < 10:
        return f"{s:.2f} s"
    return f"{s:.1f} s"


def esc(t: str) -> str:
    return t.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


parts = []
y = 64
for corpus in RESULTS["corpora"]:
    times = corpus["times"]
    slowest = max(times[k] for k, _ in TOOLS)
    plot_w = WIDTH - LEFT - LABEL_W - VALUE_W
    ours = times["py-doc"]
    parts.append(
        f'<text class="h" x="{LEFT}" y="{y}">{esc(corpus["name"])}</text>'
        f'<text class="s" x="{LEFT}" y="{y + 17}">{esc(corpus["detail"])}</text>'
    )
    row_y = y + PANEL_TOP - 12
    for key, label in TOOLS:
        t = times[key]
        w = max(3.0, plot_w * t / slowest)
        x0 = LEFT + LABEL_W
        cls = "acc" if key == "py-doc" else "mut"
        parts.append(
            f'<text class="{"l b" if key == "py-doc" else "l"}" x="{x0 - 10}" '
            f'y="{row_y + BAR_H / 2 + 4.5}" text-anchor="end">{esc(label)}</text>'
        )
        # Rounded data-end, square at the baseline.
        r = min(4.0, w / 2)
        parts.append(
            f'<path class="{cls}" d="M{x0},{row_y} h{w - r:.2f} '
            f"a{r},{r} 0 0 1 {r},{r} v{BAR_H - 2 * r} a{r},{r} 0 0 1 -{r},{r} "
            f'h-{w - r:.2f} z"><title>{esc(label)}: {fmt_time(t)}</title></path>'
        )
        value = fmt_time(t)
        if key != "py-doc":
            value += f"  ·  {t / ours:,.0f}× slower"
        parts.append(
            f'<text class="{"v b" if key == "py-doc" else "v"}" x="{x0 + w + 8:.2f}" '
            f'y="{row_y + BAR_H / 2 + 4.5}">{esc(value)}</text>'
        )
        row_y += BAR_H + ROW_GAP
    parts.append(
        f'<line class="axis" x1="{LEFT + LABEL_W}" x2="{LEFT + LABEL_W}" '
        f'y1="{y + PANEL_TOP - 16}" y2="{row_y - ROW_GAP + 4}"/>'
    )
    y = row_y + PANEL_GAP

height = y - PANEL_GAP + 50
# Footer on two lines: the method, then how to reproduce.
method, _, reproduce = RESULTS["machine"].partition(" Reproduce: ")
foot = (
    f'<text class="f" x="{LEFT}" y="{height - 28}">{esc(method)}</text>'
    f'<text class="f" x="{LEFT}" y="{height - 11}">Reproduce: {esc(reproduce)}</text>'
)

svg = f"""<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}" role="img" aria-labelledby="t d">
<title id="t">Time to generate HTML API docs: py-doc vs pydoc, pdoc3, pdoc and Sphinx</title>
<desc id="d">{esc("; ".join(c["name"] + ": " + ", ".join(f"{lbl} {fmt_time(c['times'][k])}" for k, lbl in TOOLS) for c in RESULTS["corpora"]))}</desc>
<style>
  .t {{ font: 600 17px -apple-system, "Segoe UI", Helvetica, Arial, sans-serif; fill: #0b0b0b; }}
  .h {{ font: 600 14px -apple-system, "Segoe UI", Helvetica, Arial, sans-serif; fill: #0b0b0b; }}
  .s, .f {{ font: 12px -apple-system, "Segoe UI", Helvetica, Arial, sans-serif; fill: #52514e; }}
  .l, .v {{ font: 13px -apple-system, "Segoe UI", Helvetica, Arial, sans-serif; fill: #52514e; }}
  .b {{ font-weight: 600; fill: #0b0b0b; }}
  .acc {{ fill: #2a78d6; }}
  .mut {{ fill: #b9b8b2; }}
  .axis {{ stroke: #d9d8d3; stroke-width: 1; }}
  @media (prefers-color-scheme: dark) {{
    .t, .h, .b {{ fill: #f0efec; }}
    .s, .f, .l, .v {{ fill: #c3c2b7; }}
    .acc {{ fill: #3987e5; }}
    .mut {{ fill: #5c5b56; }}
    .axis {{ stroke: #3a3936; }}
  }}
</style>
<text class="t" x="{LEFT}" y="26">Time to generate HTML API docs (lower is better)</text>
{chr(10).join(parts)}
{foot}
</svg>
"""
OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text(svg)
print("wrote", OUT)
