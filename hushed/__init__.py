"""hushed: run a Python script unchanged and see what actually happened, including the errors it
swallowed.

The tool is written in Rust (see src/ in the repository) and compiled into `hushed._native`;
this package only loads it.
"""
from importlib.metadata import PackageNotFoundError, version

try:
    __version__ = version("hushed")
except PackageNotFoundError:  # a source checkout that was never installed
    __version__ = "unknown"
