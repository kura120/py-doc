"""Reproducible doc-generator benchmark: py-doc vs pydoc, pdoc, pdoc3 and Sphinx.

Requirements: Python 3.10+, hyperfine, and a release build of py-doc
(`cargo build --release`). Everything else is installed into ./.bench/.

    python3 benchmarks/bench.py            # run, write benchmarks/results.json
    python3 benchmarks/make_chart.py       # redraw benchmarks/benchmark.svg

Fairness rules:
* Every tool documents the same packages, installed from PyPI into the same
  Python. Import-based tools (pydoc, pdoc, pdoc3, Sphinx) only get modules
  that import cleanly on this machine; py-doc never imports, so it parses
  every file and does slightly more work.
* Output directories are removed before every run (cold build every time).
* Sphinx is timed for sphinx-apidoc + sphinx-build with `-j auto`, so it uses
  every core just like py-doc. pdoc, pdoc3 and pydoc are single-process.
"""

import json
import os
import pathlib
import platform
import shutil
import statistics
import subprocess
import sys
import textwrap

REPO = pathlib.Path(__file__).resolve().parent.parent
WORK = REPO / ".bench"
# pdoc and pdoc3 both install a `pdoc` executable, so each tool gets its own venv.
VENVS = {"pdoc": WORK / "venv-pdoc", "pdoc3": WORK / "venv-pdoc3", "sphinx": WORK / "venv-sphinx"}
OUT = WORK / "out"
CMDS = WORK / "cmds"
PY_DOC = REPO / "target" / "release" / ("py-doc.exe" if os.name == "nt" else "py-doc")

TOOLS = {"pdoc": "pdoc==16.0.0", "pdoc3": "pdoc3==0.11.6", "sphinx": "sphinx==9.2.0"}
CORPORA = {
    "requests": "requests==2.34.2",
    "rich": "rich==15.0.0",
    "sqlalchemy": "SQLAlchemy==2.1.6",
}
# Modules that import fine on their own but break pdoc's import of the tree.
PDOC_EXCLUDES = {
    "sqlalchemy": ["sqlalchemy.testing.plugin.bootstrap", "sqlalchemy.testing.plugin.pytestplugin"],
}
RUNS = {"py-doc": 20, "default": 5, ("sphinx", "sqlalchemy"): 3}


def sh(*args, **kw):
    return subprocess.run(args, check=True, **kw)


def setup() -> pathlib.Path:
    for tool, venv in VENVS.items():
        if not venv.exists():
            sh(sys.executable, "-m", "venv", str(venv))
        sh(str(venv / "bin" / "pip"), "install", "-q", TOOLS[tool], *CORPORA.values())
    out = subprocess.check_output(
        [str(VENVS["pdoc"] / "bin" / "python"), "-c", "import sysconfig;print(sysconfig.get_paths()['purelib'])"],
        text=True,
    )
    return pathlib.Path(out.strip())


def importable_modules(site: pathlib.Path, pkg: str) -> list[str]:
    """Every module in the package that imports cleanly in the bench venv."""
    code = textwrap.dedent(f"""
        import importlib, pathlib, warnings, json
        warnings.simplefilter("ignore")
        root = pathlib.Path({str(site)!r})
        mods = []
        for f in sorted((root / {pkg!r}).rglob("*.py")):
            parts = list(f.relative_to(root).with_suffix("").parts)
            if parts[-1] == "__main__":
                continue
            if parts[-1] == "__init__":
                parts = parts[:-1]
            name = ".".join(parts)
            try:
                importlib.import_module(name)
                mods.append(name)
            except BaseException:
                pass
        print(json.dumps(mods))
    """)
    out = subprocess.check_output([str(VENVS["pdoc"] / "bin" / "python"), "-c", code], text=True,
                                  stderr=subprocess.DEVNULL)
    excluded = set(PDOC_EXCLUDES.get(pkg, []))
    return [m for m in json.loads(out.strip().splitlines()[-1]) if m not in excluded]


def script(name: str, body: str) -> pathlib.Path:
    path = CMDS / name
    path.write_text("#!/bin/bash\nset -e\n" + textwrap.dedent(body))
    path.chmod(0o755)
    return path


def commands(site: pathlib.Path, pkg: str) -> dict[str, pathlib.Path]:
    pdoc_bin = VENVS["pdoc"] / "bin"
    pdoc3_bin = VENVS["pdoc3"] / "bin"
    sphinx_bin = VENVS["sphinx"] / "bin"
    mods_file = CMDS / f"{pkg}.mods"
    mods_file.write_text("\n".join(importable_modules(site, pkg)))
    excludes = " ".join(f"'!{m}'" for m in PDOC_EXCLUDES.get(pkg, []))
    src = WORK / f"sphinx-src-{pkg}"
    return {
        "py-doc": script(f"py-doc-{pkg}.sh", f"""
            rm -rf {OUT}/py-doc-{pkg}
            {PY_DOC} --src {site}/{pkg} --out {OUT}/py-doc-{pkg} --name {pkg} >/dev/null 2>&1
        """),
        "pydoc": script(f"pydoc-{pkg}.sh", f"""
            rm -rf {OUT}/pydoc-{pkg}; mkdir -p {OUT}/pydoc-{pkg}; cd {OUT}/pydoc-{pkg}
            {pdoc_bin}/python - >/dev/null 2>&1 <<'EOF'
import pydoc
for m in open("{mods_file}").read().split():
    try:
        pydoc.writedoc(m)
    except Exception:
        pass
EOF
        """),
        "pdoc": script(f"pdoc-{pkg}.sh", f"""
            rm -rf {OUT}/pdoc-{pkg}
            {pdoc_bin}/pdoc --no-show-source -o {OUT}/pdoc-{pkg} $(cat {mods_file}) {excludes} >/dev/null 2>&1
        """),
        "pdoc3": script(f"pdoc3-{pkg}.sh", f"""
            rm -rf {OUT}/pdoc3-{pkg}
            {pdoc3_bin}/pdoc --html --skip-errors -f -o {OUT}/pdoc3-{pkg} {pkg} >/dev/null 2>&1 || true
        """),
        "sphinx": script(f"sphinx-{pkg}.sh", f"""
            rm -rf {OUT}/sphinx-{pkg} {src}; mkdir -p {src}
            cat > {src}/conf.py <<'EOF'
project = "{pkg}"
extensions = ["sphinx.ext.autodoc", "sphinx.ext.napoleon"]
autodoc_default_options = {{"members": True, "undoc-members": True, "show-inheritance": True}}
EOF
            printf '{pkg}\\n====\\n\\n.. toctree::\\n\\n   modules\\n' > {src}/index.rst
            {sphinx_bin}/sphinx-apidoc -q -o {src} {site}/{pkg} >/dev/null 2>&1
            {sphinx_bin}/sphinx-build -q -E -j auto -b html {src} {OUT}/sphinx-{pkg} >/dev/null 2>&1 || true
        """),
    }


def median_seconds(report: pathlib.Path) -> float:
    """Median wall time from a hyperfine JSON export (1.x or 2.x schema)."""
    result = json.loads(report.read_text())["results"][0]
    if "times" in result:
        times = result["times"]
    else:
        times = [m["time_wall_clock"]["value"] for m in result["measurements"]]
    return statistics.median(times)


def main() -> None:
    if not PY_DOC.exists():
        sys.exit("build py-doc first: cargo build --release")
    if not shutil.which("hyperfine"):
        sys.exit("hyperfine is required: https://github.com/sharkdp/hyperfine")
    CMDS.mkdir(parents=True, exist_ok=True)
    site = setup()

    corpora = []
    for pkg in CORPORA:
        files = list((site / pkg).rglob("*.py"))
        lines = sum(len(f.read_text(errors="ignore").splitlines()) for f in files)
        times = {}
        for tool, cmd in commands(site, pkg).items():
            runs = RUNS.get((tool, pkg), RUNS.get(tool, RUNS["default"]))
            report = WORK / f"{tool}-{pkg}.json"
            sh("hyperfine", "-N", "-w", "1", "-r", str(runs), "--export-json", str(report), str(cmd))
            times[tool] = median_seconds(report)
        corpora.append({
            "name": f"{pkg} {CORPORA[pkg].split('==')[1]}",
            "detail": f"{len(files)} files, {lines:,} lines of Python",
            "times": times,
        })

    machine = f"Median of 3-20 cold runs. {platform.system()}, {os.cpu_count()} cores, Python {platform.python_version()}. Reproduce: python3 benchmarks/bench.py"
    (REPO / "benchmarks" / "results.json").write_text(
        json.dumps({"machine": machine, "corpora": corpora}, indent=2) + "\n"
    )
    print("wrote benchmarks/results.json")


if __name__ == "__main__":
    main()
