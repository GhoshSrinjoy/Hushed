"""Read hushed's JSON report from Python: list the swallowed errors, and fail if there are any.

    HUSHED_JSON=report.json python examples/app/main.py
    python examples/check_report.py report.json

`import hushed` never changes a program's exit code, so a CI job can run this check afterwards
(`hushed run --fail-on-swallowed` does the same for `hushed run`). The format is in docs/report.md.
"""
import json
import sys


def main(path):
    with open(path, encoding="utf-8") as f:
        run = json.load(f)["run"]
    hidden = [s for s in run["swallowed"] if s["hidden"]]
    for s in hidden:
        at = s["caught"]
        print(f"{at['file']}:{at['line']}: {s['type']}: {s['message']}  (x{s['count']}, {s['reason']})")
    print(f"{sum(s['count'] for s in hidden)} swallowed error(s)" if hidden else "no swallowed errors")
    return 1 if hidden else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
