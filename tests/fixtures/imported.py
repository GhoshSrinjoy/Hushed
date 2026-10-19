"""Runs cases.py watched by `import hushed` instead of `hushed run`. Used by tests/test_import.py."""
import hushed  # noqa: F401  (first, so everything after it is watched)

import os
import runpy

runpy.run_path(os.path.join(os.path.dirname(__file__), "cases.py"), run_name="__main__")
