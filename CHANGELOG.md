# Changelog

## 0.3.0 (unreleased)

First release as `hushed`.

- `hushed run` runs a script or module unchanged and reports every error that was raised and then
  swallowed: where it was raised, where it was caught, and what the `except` block did with it.
  It also reports time per function and the functions that never ran.
- `hushed analyze` reads source files without running them and lists functions with their
  complexity, and the `except` blocks that can hide errors.
- `--json` writes either report as JSON; `--fail-on-swallowed` turns swallowed errors into a
  failing exit code for CI.
- Written in Rust (PyO3, stable ABI), so one wheel per platform covers Python 3.12 and later.
  No runtime dependencies.
- Replaces the earlier `analyzer.py` and `rich_logger.py` tools.
