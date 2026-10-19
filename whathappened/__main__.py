"""Command line: `whathappened run script.py` and `whathappened analyze file.py`."""
import sys

SKIP_DIRS = {"__pycache__", "venv", "env", "node_modules", "build", "dist", "site-packages"}


def main(argv=None) -> int:
    if sys.version_info < (3, 12):
        sys.stderr.write(
            "whathappened needs Python 3.12 or newer (it is built on sys.monitoring); "
            f"this is Python {sys.version.split()[0]}.\n"
            "Run it with a newer interpreter, e.g. `python3.13 -m whathappened run script.py` "
            "(on Windows: `py -3.13 -m whathappened run script.py`).\n")
        return 2

    import argparse

    from . import __version__

    parser = argparse.ArgumentParser(
        prog="whathappened",
        description="Run a Python script unchanged and see what actually happened, "
                    "including the errors it swallowed.")
    parser.add_argument("--version", action="version", version=f"%(prog)s {__version__}")
    sub = parser.add_subparsers(dest="command", required=True)

    run = sub.add_parser("run", help="run a script and report what happened",
                         description="Run a script (or module) unchanged, then report swallowed "
                                     "errors, time per function and functions that never ran. "
                                     "The report goes to stderr; the script's own output is untouched.")
    run.add_argument("-m", dest="module", action="store_true", help="run a module, like `python -m`")
    run.add_argument("--all", action="store_true", help="also list errors that were caught on purpose")
    run.add_argument("--engine", choices=["auto", "rust", "python"], default="auto",
                     help="rust (compiled, fastest) or python (no build needed); auto picks rust if built")
    run.add_argument("--exceptions-only", action="store_true",
                     help="skip timing, so the script runs at full speed")
    run.add_argument("--fail-on-swallowed", action="store_true",
                     help="exit with code 3 when errors were swallowed but the script itself succeeded")
    run.add_argument("--include", action="append", default=[], metavar="DIR",
                     help="also watch code under DIR (default: the script's folder)")
    run.add_argument("--json", metavar="FILE", help="also write the report as JSON to FILE")
    run.add_argument("--top", type=int, default=10, metavar="N",
                     help="how many of the slowest functions to show (default 10)")
    run.add_argument("target", help="script path, or module name with -m")
    run.add_argument("args", nargs=argparse.REMAINDER, help="arguments passed on to the script")

    analyze = sub.add_parser("analyze", help="read source files without running them",
                             description="List functions (with complexity) and the except blocks "
                                         "that can hide errors.")
    analyze.add_argument("paths", nargs="+", metavar="PATH", help="Python files or folders")
    analyze.add_argument("-v", "--verbose", action="store_true",
                         help="also list print() calls and assigned variables")
    analyze.add_argument("--json", metavar="FILE", help="also write the report as JSON to FILE")

    opts = parser.parse_args(argv)
    return _run(opts) if opts.command == "run" else _analyze(opts)


def _run(opts) -> int:
    import os

    from . import report
    from .tracer import Tracer

    if opts.module:
        roots = [os.getcwd()]
    elif os.path.isdir(opts.target):
        roots = [os.path.abspath(opts.target)]
    elif os.path.isfile(opts.target):
        roots = [os.path.dirname(os.path.abspath(opts.target))]
    else:
        sys.stderr.write(f"whathappened: can't open file {opts.target!r}: no such file\n")
        return 2
    try:
        tracer = Tracer(roots + opts.include, timing=not opts.exceptions_only, engine=opts.engine)
    except ValueError as e:
        sys.stderr.write(f"whathappened: {e}\n")
        return 2
    result = tracer.run(opts.target, opts.args, module=opts.module)
    try:
        sys.stdout.flush()      # the script's own output first, then the report
    except Exception:
        pass
    report.print_run(result, show_all=opts.all, top=opts.top)
    if opts.json:
        _write_json(opts.json, report.run_json(result))
    if opts.fail_on_swallowed and result.exit_code == 0 and any(s.hidden for s in result.swallowed):
        return 3
    return result.exit_code


def _analyze(opts) -> int:
    from . import report, static

    files = [f for p in opts.paths for f in _python_files(p)]
    if not files:
        sys.stderr.write("whathappened: no Python files found\n")
        return 2
    reports = [static.analyze_file(f) for f in files]
    report.print_analysis(reports, verbose=opts.verbose)
    if opts.json:
        _write_json(opts.json, report.analysis_json(reports))
    return 1 if any(r.error for r in reports) else 0


def _python_files(path: str) -> list[str]:
    import os

    if not os.path.isdir(path):
        return [path]
    found = []
    for folder, dirs, files in os.walk(path):
        dirs[:] = sorted(d for d in dirs if not d.startswith(".") and d not in SKIP_DIRS)
        found += [os.path.join(folder, f) for f in sorted(files) if f.endswith(".py")]
    return found


def _write_json(path: str, data: dict) -> None:
    import json

    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)
        f.write("\n")


if __name__ == "__main__":
    sys.exit(main())
