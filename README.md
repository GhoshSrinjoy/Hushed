# whathappened

**Your script said it worked. Here's what it hid.**

`whathappened` runs any Python script unchanged and reports every error that was raised and then
swallowed: where it was raised, where it was caught, and what the `except` block did with it. It also
shows time per function and the functions that never ran.

It is a Python package written in Rust. It's light enough to leave on, and has no dependencies.

```text
$ whathappened run examples/test_script.py
...the script's own output...

── whathappened: examples/test_script.py ───────────────────────────
finished normally in 0.31s

✗ 2 errors were swallowed (the run looked fine)
  ValueError: Division by zero!
      raised  complex_operation  test_script.py:73
      caught  main               test_script.py:114  ← `except Exception` only printed the message; traceback lost
  ValueError: Unknown operation: invalid
      raised  complex_operation  test_script.py:76
      caught  main               test_script.py:108  ← `except Exception` only printed the message; traceback lost

Time per function  (includes the calls it makes)
      0.301s  main                                     1 call
      0.301s  complex_operation                        5 calls
      0.000s  calculate_fibonacci                      25 calls
      0.000s  process_data                             1 call
      0.000s  process_data.<locals>.calculate_average  1 call
```

## Why

Scripts are full of `try: ... except Exception as e: print(e)`. Such scripts exit 0 and print their
results while part of the work quietly failed. Linters can flag the pattern, but they can't tell you
which of those blocks fired in this run, or with what error. Error trackers need setup and never see
a `print(e)`. Profilers ignore exceptions.

## Install

Python 3.12 or newer.

```bash
pip install .            # from a checkout; compiles the Rust part (needs Rust: https://rustup.rs)
```

A built wheel (see [Development](#development)) installs without Rust.

## Usage

```bash
whathappened run script.py [args...]      # run a script, report to stderr
whathappened run -m package.module        # run a module, like python -m
whathappened analyze src/                 # read files without running them
```

`python -m whathappened ...` works the same way.

Options for `run` (they go before the script; everything after it is passed to the script):

| Option | Effect |
| --- | --- |
| `--all` | also list errors caught on purpose (fallbacks, logged with traceback, `suppress`) |
| `--fail-on-swallowed` | exit with code 3 when errors were swallowed but the script succeeded (for CI and cron jobs) |
| `--json FILE` | also write the full report as JSON, for CI and other tools |
| `--exceptions-only` | skip timing; the script runs at full speed |
| `--include DIR` | also watch code under `DIR` (default: the script's folder; installed packages are never watched) |
| `--top N` | how many of the slowest functions to show (default 10) |

The report goes to stderr, so the script's own stdout stays clean (`whathappened run job.py > out.csv`
works). If the script crashes, Python's normal traceback is still printed.

| Exit code | Meaning |
| --- | --- |
| the script's own | 0, or whatever it passed to `sys.exit()` |
| 1 | the script crashed |
| 130 | the script was interrupted with Ctrl+C |
| 3 | with `--fail-on-swallowed`: the script succeeded, but errors were swallowed |
| 2 | whathappened could not start: bad arguments, no such file, or Python older than 3.12 |

`analyze` lists every function with its McCabe complexity, and every `except` block that can hide
errors. Add `-v` to also list `print()` calls and assigned variables per scope, and `--json FILE` to
write the result as JSON. It exits with 1 if a file has a syntax error.

## What counts as swallowed

An error counts as swallowed if it was caught and the program carried on without it. `whathappened`
reads whatever caught it:

| What caught it | The report says |
| --- | --- |
| an `except` with only `pass`, `continue`, `break` or `return None` | silently ignored |
| an `except` that only prints the message | only printed the message; traceback lost |
| an `except` that logs the message without `exc_info` | logged the message only; traceback lost |
| a bare `except:` or `except Exception` that returns a value or assigns a default | took a fallback path |
| a `finally` block that runs `return`, `break` or `continue` | dropped by a `finally` block |
| the thread machinery: the error ended a thread while the program kept going | killed its thread |
| library code, e.g. a framework or `concurrent.futures` | caught inside library code |
| an `except` that logs with traceback, a fallback for one specific exception type, or `with suppress(...)` | listed under "Caught on purpose" (with `--all`) |
| an `except` that re-raises, or raises a new error from it | nothing: the error is still visible |

For `except` blocks the report starts with the clause itself, for example "`except Exception` only
printed the message; traceback lost". `StopIteration`,
`StopAsyncIteration` and `GeneratorExit` are control flow and are never reported.

## How it works

- **Nothing in your code is changed.** It uses `sys.monitoring` (PEP 669, Python 3.12+). There are
  no decorators and no rewritten copies of your files, and `__future__` imports, pickling, `__main__`
  guards and `sys.argv` behave exactly as with plain `python`.
- **Written in Rust.** The hooks that run on every call, return and exception, the source analyzer
  (tree-sitter's Python grammar), the report and the command line are compiled into one module with
  PyO3. The Python package is a short launcher that checks the interpreter version and calls it.
- **It never holds on to an exception.** It follows each exception by its address, never by a
  reference, so frames, open files and locals are freed exactly when they would be without the tool.
- **Library code is cheap.** The per-call hooks are switched off for every code location outside your
  project after their first event. Only exception events still arrive, and those are rare.

## Speed

Worst case: a recursive `fib(25)` makes 242,785 tiny calls (Python 3.13, Windows).

| Run | Time | Extra per call |
| --- | --- | --- |
| plain `python` | 0.006 s | – |
| `whathappened run` | 0.033 s | 0.11 µs |
| `python -m cProfile` (C) | 0.035 s | 0.12 µs |
| `whathappened run --exceptions-only` | 0.006 s | none measurable |

Functions that do real work (I/O, network calls, parsing) make the overhead a rounding error. For
comparison, the old Rich-based logger in this repository added about 1,300 µs to every call.

Tested on Windows 11 (Python 3.12 and 3.13) and Ubuntu 24.04 (Python 3.12). Not tested on macOS yet.

## Limitations

- Only the main process is watched, not subprocesses or `multiprocessing` workers.
- Errors caught inside C code (for example `hasattr`, or `getattr` with a default) are invisible.
- With Python's default C implementation of asyncio, errors stored in tasks that are never awaited
  aren't seen (asyncio itself warns about those at exit).
- Time per function counts only time actually running. Time a generator or coroutine spends
  suspended is not included.

## Development

```bash
python -m venv .venv
source .venv/bin/activate              # Windows: .venv\Scripts\activate
pip install maturin
maturin develop --release              # compile the Rust part into the package
python -m unittest discover -s tests   # end-to-end tests that drive the real command
cargo clippy --release                 # lint the Rust code
```

`maturin build --release` builds a wheel for this machine. To build a Linux wheel from another OS,
install zig (`pip install ziglang`) and run `maturin build --release --target x86_64-unknown-linux-gnu --zig`.

| Path | What it holds |
| --- | --- |
| [src/cli.rs](src/cli.rs) | the command line |
| [src/tracer.rs](src/tracer.rs) | runs the script under `sys.monitoring` and builds the report data |
| [src/monitor.rs](src/monitor.rs) | the hooks that run on every call, return and exception |
| [src/source.rs](src/source.rs) | source analysis: functions, complexity, what each `except` block does |
| [src/report.rs](src/report.rs) | text and JSON output |
| [src/paths.rs](src/paths.rs) | which files count as your code |
| [whathappened/](whathappened/) | the Python package: just the launcher |
| [tests/](tests/) | end-to-end tests |
| [examples/](examples/) | the demo above, and a heavier one that needs `sentence-transformers` |

This replaces the earlier `analyzer.py` / `rich_logger.py` tools. The runtime monitor never wrapped
any functions, and the instrumented copies printed `None` before every line and broke on
`__future__` imports. `whathappened` covers both jobs: `run` for runtime behaviour and `analyze` for
static structure.

## License

MIT, see [LICENSE](LICENSE).
