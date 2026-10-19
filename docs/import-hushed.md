# `import hushed`

Add one line at the top of your program and run it the way you always do. When the program exits,
hushed prints a report to stderr: the errors that were swallowed, where each one was raised and
caught, and the time spent in each function. Your program's own output, exit code and behaviour
stay the same.

Every example on this page is a file in [`examples/`](../examples/), and every output is exactly
what it prints. They're produced by running the examples (`python scripts/update_docs.py`).

- [Quick start](#quick-start)
- [A whole program: modules and threads](#a-whole-program-modules-and-threads)
- [When the program crashes](#when-the-program-crashes)
- [Options](#options)
- [Reading the report from Python](#reading-the-report-from-python)
- [Where to put the import, and what is watched](#where-to-put-the-import-and-what-is-watched)

## Quick start

[`examples/quickstart.py`](../examples/quickstart.py):

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

Run it as usual:

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

- The first line is the program's own output, and it looks like success.
- The report says `load_config` swallowed a `FileNotFoundError`: `config.json` doesn't exist, and
  `except Exception` quietly returned `{}`, so the job ran on defaults nobody chose.
- `retries` caught a `KeyError` too, but with a handler for that specific type. That's an expected
  error, so it's only counted, as caught on purpose. `HUSHED_ALL=1` lists it (see [Options](#options)).
- Each function's time includes the calls it makes.

## A whole program: modules and threads

The import in the file you run covers every module the program imports and every thread it starts.
[`examples/app/`](../examples/app/) is a small app in three files:

<!-- file: examples/app/main.py -->
```python
"""A small app: a few modules and a thread pool. One line at the top watches all of it.

    python examples/app/main.py

The app says every user was synced. The report hushed prints when the app exits says otherwise.
"""
import hushed  # noqa: F401  (first, so everything after it is watched: every module, every thread)

from concurrent.futures import ThreadPoolExecutor

import remote
import users


def main():
    people = users.load()
    with ThreadPoolExecutor(max_workers=4) as pool:
        synced = list(pool.map(remote.push, people))
    print(f"synced {len(synced)} users")


if __name__ == "__main__":
    main()
```

<!-- file: examples/app/users.py -->
```python
"""Loads the users to sync."""
import json

BUILT_IN = [
    {"id": 1, "email": "ada@example.com"},
    {"id": 2, "email": "grace@example"},
    {"id": 3, "email": None},
]


def load(path="users.json"):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return BUILT_IN  # the file is missing, so a built-in list is synced instead, without a word
```

<!-- file: examples/app/remote.py -->
```python
"""Sends users to a (pretend) remote service. Runs in the app's worker threads."""


def check(user):
    domain = user["email"].split("@")[-1]
    if "." not in domain:
        raise ValueError(f"invalid email for user {user['id']}: {user['email']}")


def push(user):
    try:
        check(user)
        # ... send the user to the service here ...
    except Exception:
        pass  # a failed push looks exactly like a successful one
    return user["id"]
```

<!-- output: python examples/app/main.py -->
```text
$ python examples/app/main.py
synced 3 users

── hushed: examples/app/main.py ────────────────────────────────────
finished normally in 0.01s

✗ 3 errors were swallowed (the run looked fine)
  AttributeError: 'NoneType' object has no attribute 'split'
      raised  check  remote.py:5
      caught  push   remote.py:14  ← `except Exception` silently ignored
  ValueError: invalid email for user 2: grace@example
      raised  check  remote.py:7
      caught  push   remote.py:14  ← `except Exception` silently ignored
  FileNotFoundError: [Errno 2] No such file or directory: 'users.json'
      raised  load  users.py:13
      caught  load  users.py:15  ← `except Exception` took a fallback path

Time per function  (includes the calls it makes)
      0.001s  main   1 call
      0.000s  load   1 call
      0.000s  push   3 calls
      0.000s  check  3 calls
```

The app says it synced 3 users. The report shows that two of the pushes failed inside the worker
threads, and that the users came from a built-in list because `users.json` doesn't exist.

## When the program crashes

[`examples/crash.py`](../examples/crash.py):

<!-- file: examples/crash.py -->
```python
"""A crash whose real cause was swallowed earlier. Python's traceback shows the KeyError; hushed's
report also shows the error that led to it.

    python examples/crash.py
"""
import hushed  # noqa: F401  (first, so everything after it is watched)

import json


def load_config(path="config.json"):
    try:
        with open(path) as f:
            return json.load(f)
    except Exception:
        return {}  # the file is missing, so the config is silently empty


def connect(config):
    return f"connecting to {config['database_url']}"


print(connect(load_config()))
```

<!-- output: python examples/crash.py -->
```text
$ python examples/crash.py
Traceback (most recent call last):
  File "examples/crash.py", line 23, in <module>
    print(connect(load_config()))
          ~~~~~~~^^^^^^^^^^^^^^^
  File "examples/crash.py", line 20, in connect
    return f"connecting to {config['database_url']}"
                            ~~~~~~^^^^^^^^^^^^^^^^
KeyError: 'database_url'

── hushed: examples/crash.py ───────────────────────────────────────
crashed in 0.00s: KeyError: 'database_url' at connect (crash.py:20)

✗ 1 error was swallowed
  FileNotFoundError: [Errno 2] No such file or directory: 'config.json'
      raised  load_config  crash.py:13
      caught  load_config  crash.py:15  ← `except Exception` took a fallback path

Time per function  (includes the calls it makes)
      0.000s  load_config  1 call
      0.000s  connect      1 call
```

Python's traceback comes first, unchanged, and the exit code is still 1. The traceback blames a
`KeyError` in `connect`. The report also shows the cause: `config.json` was missing, and
`load_config` swallowed that.

When a program ends with `sys.exit(n)`, the status line says `exited with code n`.

## Options

Options are environment variables, so changing them never needs a code change. The examples below
use bash. In PowerShell, set the variable first and remove it afterwards:

```powershell
$env:HUSHED_ALL = "1"; python examples/quickstart.py; Remove-Item Env:HUSHED_ALL
```

| Variable | What it does |
| --- | --- |
| `HUSHED=0` | Turn hushed off without removing the import. |
| `HUSHED_ALL=1` | Also list the errors caught on purpose, and the functions that never ran in every watched file. |
| `HUSHED_EXCEPTIONS_ONLY=1` | Skip timing: the program runs at full speed and only swallowed errors are reported. |
| `HUSHED_JSON=FILE` | Also write the report to `FILE` as JSON ([format](report.md#json-format)). |
| `HUSHED_INCLUDE=DIR` | Also watch code under `DIR`. Separate several folders with `:` (`;` on Windows). |
| `HUSHED_TOP=N` | How many functions the timing table shows (default 10). |

### See everything: `HUSHED_ALL=1`

<!-- output: HUSHED_ALL=1 python examples/quickstart.py -->
```text
$ HUSHED_ALL=1 python examples/quickstart.py
job finished with 3 retries

── hushed: examples/quickstart.py ──────────────────────────────────
finished normally in 0.00s

✗ 1 error was swallowed (the run looked fine)
  FileNotFoundError: [Errno 2] No such file or directory: 'config.json'
      raised  load_config  quickstart.py:12
      caught  load_config  quickstart.py:14  ← `except Exception` took a fallback path

Caught on purpose: 1 error
  KeyError: 'retries'
      raised  retries  quickstart.py:20
      caught  retries  quickstart.py:21  ← `except KeyError` took a fallback path

Time per function  (includes the calls it makes)
      0.000s  load_config  1 call
      0.000s  retries      1 call
```

### Errors only, at full speed: `HUSHED_EXCEPTIONS_ONLY=1`

<!-- output: HUSHED_EXCEPTIONS_ONLY=1 python examples/quickstart.py -->
```text
$ HUSHED_EXCEPTIONS_ONLY=1 python examples/quickstart.py
job finished with 3 retries

── hushed: examples/quickstart.py ──────────────────────────────────
finished normally in 0.00s

✗ 1 error was swallowed (the run looked fine)
  FileNotFoundError: [Errno 2] No such file or directory: 'config.json'
      raised  load_config  quickstart.py:12
      caught  load_config  quickstart.py:14  ← `except Exception` took a fallback path
  + 1 error was caught on purpose; show them with HUSHED_ALL=1
```

### Off: `HUSHED=0`

<!-- output: HUSHED=0 python examples/quickstart.py -->
```text
$ HUSHED=0 python examples/quickstart.py
job finished with 3 retries
```

## Reading the report from Python

`HUSHED_JSON` writes the whole report as JSON when the program exits ([format](report.md#json-format)):

<!-- run: HUSHED_JSON=report.json python examples/quickstart.py -->
```bash
HUSHED_JSON=report.json python examples/quickstart.py
```

[`examples/check_report.py`](../examples/check_report.py) reads it, lists the swallowed errors and
exits with 1 if there are any:

<!-- file: examples/check_report.py -->
```python
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
```

<!-- output: python examples/check_report.py report.json -->
```text
$ python examples/check_report.py report.json
quickstart.py:14: FileNotFoundError: [Errno 2] No such file or directory: 'config.json'  (x1, `except Exception` took a fallback path)
1 swallowed error(s)
```

`import hushed` never changes your program's exit code. To fail a CI job on swallowed errors, run a
check like this after the program, or use `hushed run --fail-on-swallowed` ([Using hushed in CI](ci.md)).

## Where to put the import, and what is watched

- Put it first, in the file you run: for `python app.py`, at the top of `app.py`. Code that ran
  before the import isn't watched.
- It watches the folder of that file (for `python -m package`, the current directory), every module
  in it, in every thread. The standard library and installed packages are never watched, but an
  error raised inside them still shows up when your code swallows it.
- `hushed run app.py` gives the same report without the import. Inside `hushed run` the import does
  nothing, so the report isn't printed twice.
- `multiprocessing` workers and forked children don't print reports of their own.
- No report is printed when the program is killed, or ends with `os._exit()`.
