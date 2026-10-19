# How it works

## Watching without changing anything

hushed uses `sys.monitoring` (PEP 669), the interpreter's monitoring hooks, added in Python 3.12.
That's why it needs 3.12 or newer. It doesn't use decorators, doesn't rewrite your files and
doesn't create copies of them. The script runs through `runpy` exactly as `python script.py` would
run it, so `__future__` imports, pickling, `if __name__ == "__main__":` and `sys.argv` all behave
as usual.

It listens to these events:

| Event | Used for |
| --- | --- |
| `PY_START`, `PY_RESUME`, `PY_THROW` | a function starting, or a generator or coroutine resuming |
| `PY_RETURN`, `PY_YIELD`, `PY_UNWIND` | a function finishing, suspending, or exiting with an exception |
| `RAISE` | an exception being raised, or passing through a frame |
| `EXCEPTION_HANDLED` | an exception landing in a handler |
| `RERAISE` | a handler (or `finally`, or a `with` exit) passing the exception on |

An exception is swallowed if it lands in a handler and isn't raised again before it's forgotten.
hushed then reads the source of whatever caught it (see [the rules](report.md#when-an-error-counts-as-swallowed)).

## Never holding on to an exception

Keeping a reference to an exception would keep its traceback alive, and with it every frame and
local variable along the way. Files would stay open longer, objects would be freed later, and the
program could behave differently. hushed follows each exception by its address only. It tells a
new exception at a reused address apart from the original by checking where that exception's
traceback starts.

## The Rust engine

Everything that runs while your script runs is Rust, compiled with PyO3 into one module that uses
Python's stable ABI. That covers the event hooks, the source analyzer (the tree-sitter Python
grammar), the report and the command line. The Python package is a short launcher that checks the
interpreter version and calls it.

Per-call hooks are switched off for each code location outside your project after its first event.
After that, library code costs nothing on calls and returns. Only exception events still arrive
from it, and those are rare.

## Overhead

Worst case: a recursive `fib(25)` makes 242,785 tiny calls (Python 3.13, Windows).

| Run | Time | Extra per call |
| --- | --- | --- |
| plain `python` | 0.006 s | – |
| `hushed run` | 0.033 s | 0.11 µs |
| `python -m cProfile` (written in C) | 0.035 s | 0.12 µs |
| `hushed run --exceptions-only` | 0.006 s | none measurable |

Real functions do real work (I/O, network calls, parsing), so in practice the overhead is a
rounding error. `--exceptions-only` costs nothing measurable, because exceptions are rare events.

## Limitations

- Only the main process is watched, not subprocesses or `multiprocessing` workers.
- An error caught inside C code (for example `hasattr()`, or `getattr()` with a default) never
  reaches a Python handler, so hushed can't see it.
- With Python's default C implementation of asyncio, an error stored in a task that is never
  awaited is invisible to hushed. asyncio itself logs those as "Task exception was never retrieved".
- hushed uses `sys.monitoring` tool id 3, or 4 if 3 is taken. If something else in the process
  uses both, `hushed run` stops with an error.
- Time per function counts only time spent running. Time a generator or coroutine spends suspended
  is not included.
