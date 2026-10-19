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

## Use it in your code: `import hushed`

Put `import hushed` first in the file you run, then run your program the way you always do. For
example, [`examples/quickstart.py`](https://github.com/GhoshSrinjoy/Hushed/blob/main/examples/quickstart.py):

<!-- file: examples/quickstart.py -->
```python
"""Quick start: `import hushed` as the first line, then run the program the usual way.

    python examples/quickstart.py
"""
import hushed  # noqa: F401  (first, so everything after it is watched)

import json


def load_config(path="config.json"):
    try:
        with open(path) as f:
            return json.load(f)
    except Exception:
        return {}  # no config file? carry on with defaults, without a word


def retries(config):
    try:
        return int(config["retries"])
    except KeyError:
        return 3  # a specific, expected error: hushed counts this one as caught on purpose


config = load_config()
print(f"job finished with {retries(config)} retries")
```

<!-- output: python examples/quickstart.py -->
```text
$ python examples/quickstart.py
job finished with 3 retries

── hushed: examples/quickstart.py ──────────────────────────────────
finished normally in 0.00s

✗ 1 error was swallowed (the run looked fine)
  FileNotFoundError: [Errno 2] No such file or directory: 'config.json'
      raised  load_config  quickstart.py:12
      caught  load_config  quickstart.py:14  ← `except Exception` took a fallback path
  + 1 error was caught on purpose; show them with HUSHED_ALL=1

Time per function  (includes the calls it makes)
      0.000s  load_config  1 call
      0.000s  retries      1 call
```

The job printed its result and exited with 0, but the report shows what it hid: `config.json` is
missing, and `except Exception` silently fell back to defaults. The `KeyError` in `retries` was
caught by a handler for that specific type, so it only counts as caught on purpose.

More, each with its code and output, in
[`import hushed`](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/import-hushed.md): a whole
app with modules and threads, a crash whose real cause was swallowed earlier, every option, and
reading the report from Python.

## Use it from the command line: `hushed run`

To watch a script without changing it, put `hushed run` in front of the command:

```bash
hushed run job.py --input data.csv      # run a script; the report goes to stderr
hushed run -m mypackage.tool            # run a module, like python -m
hushed run --fail-on-swallowed job.py   # exit with code 3 if errors were swallowed (for CI)
hushed run --json report.json job.py    # also write the report as JSON
hushed analyze src/                     # find except blocks that can hide errors, without running
```

![hushed analyze examples/pipeline.py](https://raw.githubusercontent.com/GhoshSrinjoy/Hushed/main/docs/images/analyze.svg)

## What counts as swallowed

An error counts as swallowed if a broad handler (a bare `except:`, `except Exception` or
`except BaseException`) caught it and the program carried on without it, for example when the
handler:

- only has `pass`, `continue` or `return None`
- only prints or logs the message, so the traceback is lost
- quietly returns a default

Also swallowed: an error dropped by a `finally` block that returns, an error that killed a thread
while the rest of the program kept going, and an error in a thread-pool task whose result nobody read.

A handler for a specific type (`except KeyError:`, `except FileNotFoundError: return None`) expected
that error, so it counts as caught on purpose, whatever it does; so do errors that are logged with
their traceback or re-raised. Handlers inside libraries are judged by the same rules. The
[full rules](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/report.md#when-an-error-counts-as-swallowed)
are in the docs.

## Documentation

- [Getting started](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/getting-started.md): install, first run, reading the report
- [`import hushed`](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/import-hushed.md): using it from inside your program, with examples and their output
- [Examples](https://github.com/GhoshSrinjoy/Hushed/tree/main/examples): runnable programs, each with what it prints
- [Commands](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/commands.md): every option, exit codes, environment variables
- [The report](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/report.md): how `except` blocks are judged, and the JSON format
- [Using hushed in CI](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/ci.md): fail a build when errors are swallowed
- [How it works](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/how-it-works.md): `sys.monitoring`, the Rust engine, overhead, limitations
- [Development](https://github.com/GhoshSrinjoy/Hushed/blob/main/docs/development.md): building, testing and releasing

## License

MIT, see [LICENSE](https://github.com/GhoshSrinjoy/Hushed/blob/main/LICENSE).
