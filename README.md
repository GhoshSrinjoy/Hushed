# hushed

[![CI](https://github.com/GhoshSrinjoy/Hushed/actions/workflows/ci.yml/badge.svg)](https://github.com/GhoshSrinjoy/Hushed/actions/workflows/ci.yml)
![Python 3.12+](https://img.shields.io/badge/python-3.12%2B-blue)
[![License: MIT](https://img.shields.io/badge/license-MIT-green)](https://github.com/GhoshSrinjoy/Hushed/blob/main/LICENSE)

**Your script said it worked. Here's what it hid.**

`hushed` runs any Python script unchanged and reports every error that was raised and then
swallowed: where it was raised, where it was caught, and what the `except` block did with it. It also
shows time per function and the functions that never ran.

![hushed run examples/pipeline.py](https://raw.githubusercontent.com/GhoshSrinjoy/Hushed/main/docs/images/run.svg)

## Why

Scripts are full of `try: ... except Exception as e: print(e)`. Such scripts exit 0 and print their
results while part of the work quietly failed. Linters can flag the pattern, but they can't tell you
which of those blocks fired in this run, or with what error. Error trackers need setup and never see
a `print(e)`. Profilers ignore exceptions.

## Install

```bash
pip install hushed
```

It needs Python 3.12 or newer, on Linux, macOS or Windows, and has no dependencies. It's written in
Rust and adds about 0.1 µs to each function call.

## Use it

```bash
hushed run job.py --input data.csv      # run a script; the report goes to stderr
hushed run -m mypackage.tool            # run a module, like python -m
hushed run --fail-on-swallowed job.py   # exit with code 3 if errors were swallowed (for CI)
hushed run --json report.json job.py    # also write the report as JSON
hushed analyze src/                     # find except blocks that can hide errors, without running
```

Or put one line at the top of your program's entry point, and run it the way you always do:

```python
import hushed   # watches the whole program from here on; the report is printed when it exits
```

The [examples](https://github.com/GhoshSrinjoy/Hushed/tree/main/examples) show both ways, and how to
read the JSON report from Python.

![hushed analyze examples/pipeline.py](https://raw.githubusercontent.com/GhoshSrinjoy/Hushed/main/docs/images/analyze.svg)

## What counts as swallowed

An error counts as swallowed if it was caught and the program carried on without it, for example:

- an `except` that only has `pass`, `continue` or `return None`
- an `except` that only prints or logs the message, so the traceback is lost
- a bare `except:` or `except Exception` that quietly returns a default
- a `finally` block that returns, which discards the error
- an error that killed a thread while the rest of the program kept going

Errors that are logged with their traceback, re-raised, or caught on purpose for one specific type are
not counted as swallowed. The [full rules](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/report.md#when-an-error-counts-as-swallowed)
are in the docs.

## Documentation

- [Getting started](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/getting-started.md): install, first run, reading the report
- [Commands](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/commands.md): every option, exit codes, environment variables
- [The report](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/report.md): how `except` blocks are judged, and the JSON format
- [Using hushed in CI](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/ci.md): fail a build when errors are swallowed
- [How it works](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/how-it-works.md): `sys.monitoring`, the Rust engine, overhead, limitations
- [Development](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/development.md): building, testing and releasing

## License

MIT, see [LICENSE](https://github.com/GhoshSrinjoy/Hushed/blob/main/LICENSE).
