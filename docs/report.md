# The report

## Reading `hushed run`

The report has up to five parts, written to stderr after the script ends.

**Status line.** One of:

- `finished normally in 0.31s`
- `exited with code N in 0.31s`: the script called `sys.exit(N)`
- `crashed in 0.31s: ValueError: ... at function (file.py:12)`: the line in your code nearest to
  where the uncaught error was raised. Python's usual traceback is printed above the report.
- `interrupted after 0.31s`: Ctrl+C

**Swallowed errors.** Errors that were caught and then lost, see [below](#when-an-error-counts-as-swallowed).
The headline counts each error once (the same type, raised and caught at the same lines) and, when
a loop repeated some of them, how often they happened in all: `4 errors were swallowed, 900 times
in all`. It adds "(the run looked fine)" when the script finished with exit code 0, because nothing
outside hushed would have shown a problem. Each entry looks like this:

```text
  ValueError: could not convert string to float: 'N/A'  ×12
      raised  price  pipeline.py:32
      caught  price  pipeline.py:33  ← `except Exception` only printed the message; traceback lost
```

- Errors with the same type, the same **raised** line and the same **caught** line are grouped;
  `×12` means it happened twelve times. The message is the first one.
- **raised** is the line in your code where the error appeared. For an error from a library or
  from C code (`int("x")`, `json.loads(...)`), it's the line in your code that made the call.
- **caught** is the `except` clause that actually matched. Python itself reports the first clause
  of a `try`, so hushed works out which clause applies from the exception's class.

**Caught on purpose.** Errors that were expected or handled visibly (caught by a handler for a
specific type, logged with a traceback, or suppressed), counted on one line. Use `--all` to list
them in the same format.

**Time per function.** The slowest named functions (top 10, or `--top N`). Each time includes the
calls the function makes. A recursive function is counted once, from its outermost call. Time a
generator or coroutine spends suspended doesn't count.

**Never ran.** Functions defined in the script's own file that were never called. With `--all`,
those in every watched file: a big program rarely uses all of its code, so that list is long.
Functions decorated with `@overload` or `@abstractmethod` are left out.

## When an error counts as swallowed

An error is *swallowed* when it was caught and the program carried on without it. hushed then reads
whatever caught it.

Only a **broad** handler can swallow an error: a bare `except:`, or `except Exception` or
`except BaseException` (alone or in a tuple). A handler for specific types (`except KeyError:`,
`except (OSError, ValueError):`) expected that error, so it counts as caught on purpose, whatever it
does with it. `except FileNotFoundError: return None` is how Python code checks for an optional
file, not a hidden failure.

| What caught it | The report says | Listed as |
| --- | --- | --- |
| a broad `except` with only `pass`, `continue`, `break`, `return`, `return None`, or a string or `...` | silently ignored | swallowed |
| a broad `except` that calls `print(...)`, `sys.stdout.write` or `sys.stderr.write` | only printed the message; traceback lost | swallowed |
| a broad `except` that calls `.error()`, `.warning()`, `.info()`, `.debug()`, `.critical()`, `.log()` or `warnings.warn()` without `exc_info` | logged the message only; traceback lost | swallowed |
| a broad `except` that does anything else (returns a default, appends to a list, ...) | took a fallback path | swallowed |
| any of these for specific exception types (`except KeyError: value = default`) | the same words | caught on purpose |
| an `except` that calls `.exception()`, passes `exc_info=...`, or uses `traceback.print_exc()` / `format_exc()` / `print_exception()` / `format_exception()` or `capture_exception()` | logged with traceback | caught on purpose |
| an `except` with a `raise` that didn't run this time | did not reach its `raise` this time | swallowed if the clause is broad |
| a `finally` block that runs `return`, `break` or `continue` (which discards the error) | dropped by a `finally` block | swallowed |
| a `with` block whose context manager suppressed it (`contextlib.suppress`, ...) | suppressed by a `with` block | caught on purpose |
| the thread machinery: the error ended a thread while the rest of the program kept going | killed its thread | swallowed |
| library code: the standard library, a framework, `concurrent.futures`, ... | caught inside library code, then why | by the same rules, read from the library's source |

Handlers in library code are judged like yours. A thread pool that catches a task's error with
`except BaseException` and stores it in a future nobody reads has swallowed it. `Mapping.get()`
catching your `__getitem__`'s `KeyError` has not. When a library's source can't be read, the error
is listed as caught on purpose.

These are never reported:

- an error that is re-raised, or turned into another error (`raise NewError(...) from e`): it is
  still visible, and the new error is followed instead;
- an error that crashes the script: that's the crash on the status line;
- `StopIteration`, `StopAsyncIteration` and `GeneratorExit`, which are control flow.

## Reading `hushed analyze`

For each file: a header with counts, a table of functions, and the `except` blocks that can hide
errors. These are judged by the same rules as above, without running anything.

| Column | Meaning |
| --- | --- |
| `line` | where the `def` is |
| `function` | qualified name, e.g. `Shop.total` or `main.<locals>.helper`; `async` is shown in front |
| `args` | parameter names, with `*`, `*args` and `**kwargs` as written |
| `complexity` | McCabe complexity: 1 + each `if`/`elif`, conditional expression, `for`, `while`, `except` clause, `assert`, `match` case, comprehension `for`/`if`, and each `and`/`or`. Above 10 it's highlighted. |
| `returns` | `value` if it returns a value, `yields` for a generator, `none` otherwise |
| `docstring` | whether the function has one |

With `-v` the report also lists the `print()` calls and the assigned variable names of each function.

## JSON format

`--json FILE` writes the same information for tools. Both commands write an object with `tool`
(`"hushed"`), `version`, and then `run` or `files`.

### `hushed run --json`

```json
{
  "tool": "hushed",
  "version": "0.3.0",
  "run": {
    "target": "examples/pipeline.py",
    "status": "ok",
    "exit_code": 0,
    "seconds": 0.0123,
    "timing": true,
    "error": null,
    "error_at": null,
    "swallowed": [
      {
        "type": "ValueError",
        "message": "could not convert string to float: 'N/A'",
        "count": 1,
        "raised": {"file": "pipeline.py", "line": 32, "function": "price"},
        "caught": {"file": "pipeline.py", "line": 33, "function": "price"},
        "kind": "except",
        "hidden": true,
        "handler": {
          "line": 33, "scope": "price", "catches": "Exception", "broad": true,
          "verdict": "printed", "clause": "except Exception", "hides_errors": true
        },
        "reason": "`except Exception` only printed the message; traceback lost"
      }
    ],
    "functions": [{"function": "main", "file": "pipeline.py", "line": 38, "calls": 1, "seconds": 0.0004}],
    "never_ran": [],
    "files": ["pipeline.py"],
    "summary": {"hidden_errors": 3, "caught_on_purpose": 0}
  }
}
```

| Field | Meaning |
| --- | --- |
| `status` | `"ok"`, `"exit"`, `"crash"` or `"interrupted"` |
| `exit_code` | the exit code hushed returns, without `--fail-on-swallowed` |
| `timing` | `false` with `--exceptions-only`; `functions` and `never_ran` are then empty |
| `never_ran` | functions that never ran, in the script's own file (in every watched file with `--all`) |
| `error`, `error_at` | for a crash: the error, and where in your code it was raised |
| `swallowed[].kind` | `"except"`, `"with"`, `"finally"`, `"thread"`, `"library"`, or `"unknown"` when the source couldn't be read |
| `swallowed[].hidden` | `true` for swallowed errors, `false` for errors caught on purpose |
| `swallowed[].handler` | the `except` clause that caught it (also in library code); `verdict` is one of `ignored`, `printed`, `logged-message`, `logged`, `fallback`, `reraised` |
| `files` | the watched files that ran code |
| `summary` | how many errors were swallowed (`hidden_errors`) and caught on purpose |

Locations use paths relative to the watched folder. Times are in seconds.

### `hushed analyze --json`

```json
{
  "tool": "hushed",
  "version": "0.3.0",
  "files": [
    {
      "path": "examples/pipeline.py",
      "functions": [
        {"qualname": "price", "line": 30, "first_line": 30, "is_async": false, "args": ["record"],
         "complexity": 2, "returns": "value", "docstring": false, "decorators": []}
      ],
      "handlers": [
        {"line": 33, "scope": "price", "catches": "Exception", "broad": true, "verdict": "printed",
         "clause": "except Exception", "hides_errors": true}
      ],
      "prints": [[34, "price"], [42, "main"]],
      "variables": {"<module>": ["ROWS"], "load_settings": ["f"], "price": ["e"], "main": ["records", "settings", "total"]},
      "error": null
    }
  ]
}
```

`first_line` is the line of the first decorator when there is one. `error` is set, and everything
else is empty, when a file can't be read or has a syntax error.
