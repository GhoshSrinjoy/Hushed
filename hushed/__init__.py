"""hushed: run a Python script unchanged and see what actually happened, including the errors it
swallowed.

`import hushed` at the top of a program watches it from that point on and prints the report when
the program exits; `hushed run script.py` does the same without touching the script.

The tool is written in Rust (see src/ in the repository) and compiled into `hushed._native`;
this package only loads it.
"""
import sys
from importlib.metadata import PackageNotFoundError, version

try:
    __version__ = version("hushed")
except PackageNotFoundError:  # a source checkout that was never installed
    __version__ = "unknown"

# Without 3.12 or the compiled part there is nothing to watch with; the `hushed` command explains.
if sys.version_info >= (3, 12):
    try:
        from hushed._native import watch as _watch
    except ImportError:
        pass
    else:
        _watch()
