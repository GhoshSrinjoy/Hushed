"""Shared test helpers: run the `hushed` command the way a user would."""
import glob
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(ROOT, "tests", "fixtures")
EXAMPLES = os.path.join(ROOT, "examples")

# Run from an empty folder, so `python -m hushed` imports the installed package rather than the
# source checkout. When the Rust part was built into the checkout (`maturin develop`), use that.
_WORK = tempfile.TemporaryDirectory(prefix="hushed-tests-")
ENV = {**os.environ, "PYTHONIOENCODING": "utf-8", "PYTHONDONTWRITEBYTECODE": "1", "NO_COLOR": "1"}
ENV.pop("FORCE_COLOR", None)
if glob.glob(os.path.join(ROOT, "hushed", "_native*")):
    ENV["PYTHONPATH"] = os.pathsep.join(filter(None, [ROOT, os.environ.get("PYTHONPATH")]))


def hushed(*args):
    """Run `python -m hushed ARGS...` and return the finished process."""
    return python("-m", "hushed", *args)


def python(*args, env=None):
    """Run `python ARGS...` and return the finished process."""
    return subprocess.run([sys.executable, *args], cwd=_WORK.name, env={**ENV, **(env or {})},
                          capture_output=True, text=True, encoding="utf-8")
