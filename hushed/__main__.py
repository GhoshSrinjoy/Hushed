"""Entry point for `python -m hushed` and the `hushed` command: check the Python
version, then hand over to the compiled Rust code."""
import sys


def main() -> int:
    if sys.version_info < (3, 12):
        sys.stderr.write(
            "hushed needs Python 3.12 or newer (it is built on sys.monitoring); "
            f"this is Python {sys.version.split()[0]}.\n"
            "Run it with a newer interpreter, e.g. `python3.13 -m hushed run script.py` "
            "(on Windows: `py -3.13 -m hushed run script.py`).\n")
        return 2
    try:
        from hushed._native import main as run
    except ImportError:
        sys.stderr.write("hushed: the compiled part is missing. Install it with `pip install .` "
                         "(needs Rust), or build it in place with `maturin develop --release`.\n")
        return 2
    return run(sys.argv[1:])


if __name__ == "__main__":
    sys.exit(main())
