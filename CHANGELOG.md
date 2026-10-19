# Changelog

## 0.3.0 (unreleased)

First release as `hushed`.

- `hushed run` runs a script or module unchanged and reports every error that was raised and then
  swallowed: where it was raised, where it was caught, and what the `except` block did with it.
  It also reports time per function and the functions that never ran.
- Only broad handlers (a bare `except:`, `except Exception`, `except BaseException`) count as
  swallowing an error; a handler for specific types expected it, so it counts as caught on purpose.
  Handlers inside libraries are judged by the same rules, read from the library's source (including
  frozen standard library modules).
- `import hushed` at the top of a program gives the same report when the program exits, without
  changing how it is started. Options are environment variables (`HUSHED_JSON`, `HUSHED_ALL`, ...),
  and `HUSHED=0` turns it off.
- `hushed analyze` reads source files without running them and lists functions with their
  complexity, and the `except` blocks that can hide errors.
- `--json` writes either report as JSON; `--fail-on-swallowed` turns swallowed errors into a
  failing exit code for CI.
- Written in Rust (PyO3, stable ABI), so one wheel per platform covers Python 3.12 and later.
  No runtime dependencies.
- Replaces the earlier `analyzer.py` and `rich_logger.py` tools.
