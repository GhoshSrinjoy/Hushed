# whathappened

**Your script said it worked. Here's what it hid.**

`whathappened` runs any Python script unchanged and reports every error that was raised and then
swallowed: where it was raised, where it was caught, and what the `except` block did with it. It also
shows time per function and the functions that never ran.

```text
$ whathappened run examples/test_script.py
...the script's own output...

── whathappened: examples/test_script.py ───────────────────────────
finished normally in 0.31s  (rust engine)

✗ 2 errors were swallowed (the run looked fine)
  ValueError: Division by zero!
      raised  complex_operation  test_script.py:73
      caught  main               test_script.py:114  ← `except Exception` only printed the message; traceback lost
  ValueError: Unknown operation: invalid
      raised  complex_operation  test_script.py:76
      caught  main               test_script.py:108  ← `except Exception` only printed the message; traceback lost

Time per function  (includes the calls it makes)
      0.302s  main                                     1 call
      0.301s  complex_operation                        5 calls
      0.000s  calculate_fibonacci                      25 calls
```

## Why

Scripts are full of `try: ... except Exception as e: print(e)`. Such
scripts exit 0 and print their results while part of the work quietly failed. Linters can flag the
pattern, but they can't tell you which of those blocks fired in this run, or with what error. Error
trackers need setup and never see a `print(e)`. Profilers ignore exceptions.

## Install

Requires Python 3.12 or newer.

```bash
pip install .            # builds the Rust engine (needs a Rust toolchain: https://rustup.rs)
```

Without Rust you can run straight from a checkout with `python -m whathappened ...`. It then uses
the pure-Python engine, which gives the same results but runs slower (see [Speed](#speed)).

## Usage

```bash
whathappened run script.py [args...]      # run a script, report to stderr
whathappened run -m package.module        # run a module, like python -m
whathappened analyze src/                 # read files without running them
```

Options for `run`:

| Option | Effect |
| --- | --- |
| `--all` | also list errors caught on purpose (fallbacks, logged with traceback, `suppress`) |
| `--fail-on-swallowed` | exit with code 3 when errors were swallowed but the script succeeded (for CI and cron jobs) |
| `--json FILE` | also write the full report as JSON, for CI and other tools |
| `--exceptions-only` | skip timing; the script runs at full speed |
| `--engine rust\|python` | choose the engine (default: Rust when built) |
| `--include DIR` | also watch code under `DIR` (default: the script's folder; installed packages are never watched) |
| `--top N` | how many of the slowest functions to show |

The report goes to stderr, so the script's own stdout stays clean (`whathappened run job.py > out.csv`
works). The exit code is the script's own: 0, its `sys.exit()` code, or 1 if it crashed. A crash still
prints Python's normal traceback.

`analyze` lists every function with its McCabe complexity, and every `except` block that can hide
errors. Add `-v` to also list `print()` calls and assigned variables per scope, and `--json FILE`
to write the result as JSON.

## What counts as swallowed

An error counts as swallowed if it was caught and the program carried on without it. `whathappened` reads the `except`
block that caught it:

| The handler... | Reported as |
| --- | --- |
| only has `pass` / `continue` / `return None` | silently ignored |
| only prints the message | traceback lost |
| logs the message without `exc_info` | traceback lost |
| returns or assigns something else, under `except Exception` or bare `except:` | fallback hiding the error |
| sits in a `finally` with `return` / `break` | error dropped |
| (the error killed a thread while the main program kept going) | thread died |
| logs with traceback, or takes a fallback for a specific exception type | caught on purpose (`--all`) |
| re-raises, or raises a new error from it | not reported: the error is still visible |

`StopIteration` and other control-flow exceptions are never reported.

## How it works

- **Nothing in your code is changed.** It uses `sys.monitoring` (PEP 669, Python 3.12+). There are no
  decorators and no rewritten copies of your files, and `__future__` imports, pickling, `__main__`
  guards and `sys.argv` behave exactly as with plain `python`.
- **The hot path is Rust.** The callbacks that run on every function call, return and exception are a
  compiled PyO3 module ([src/lib.rs](src/lib.rs)). [whathappened/tracer.py](whathappened/tracer.py)
  has the same engine in pure Python as a fallback, and the tests check that both give identical
  reports.
- **It never holds on to an exception.** It follows each exception by its address, never by a
  reference, so frames, open files and locals are freed exactly when they would be without the tool.
- **Library code costs nothing.** Monitoring is switched off for code outside your project after its
  first event.

## Speed

Worst case: a recursive `fib(25)` makes 242,785 tiny calls (Python 3.13, Windows).

| Run | Time | Extra per call |
| --- | --- | --- |
| plain `python` | 0.006 s | – |
| `whathappened`, Rust engine | 0.033 s | 0.11 µs |
| `python -m cProfile` (C) | 0.035 s | 0.12 µs |
| `whathappened`, Python engine | 0.249 s | 1.0 µs |
| `whathappened --exceptions-only` | 0.006 s | none measurable |

Functions that do real work (I/O, model calls, parsing) make the overhead a rounding error.

## Limitations

- Only the main process is watched, not subprocesses or `multiprocessing` workers.
- Errors caught inside C code (for example `hasattr`, or `getattr` with a default) are invisible.
- With Python's default C implementation of asyncio, errors stored in tasks that are never awaited
  aren't seen (asyncio itself warns about those at exit).
- Time per function counts only time actually running. Time a generator or coroutine spends
  suspended is not included.

## Development

```bash
cargo build --release                         # build the Rust engine...
cp target/release/_native.dll whathappened/_native.pyd   # ...and drop it into the package (.so on Linux/macOS)
python -m unittest discover -s tests          # runs every test against both engines
```

`examples/` has the demo script above and a heavier one (`sentence_similarity.py`, which needs
`sentence-transformers`).

This replaces the earlier `analyzer.py` / `rich_logger.py` tools. The runtime monitor never
wrapped any functions, and the instrumented copies printed `None` before every line and broke on
`__future__` imports. `whathappened` covers both jobs: `run` for runtime behaviour and `analyze` for
static structure.

## License

MIT, see [LICENSE](LICENSE).
