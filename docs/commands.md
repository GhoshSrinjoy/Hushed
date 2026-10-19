# Commands

`hushed` has two commands. `python -m hushed ...` works the same as `hushed ...`. Instead of
`hushed run`, a program can also [`import hushed`](#import-hushed).

## `hushed run`

```text
hushed run [OPTIONS] SCRIPT [ARGS]...
hushed run [OPTIONS] -m MODULE [ARGS]...
```

Runs a script, a folder with a `__main__.py`, or (with `-m`) a module, exactly as `python` would:
same `sys.argv`, same `sys.path[0]`, and `__name__ == "__main__"`. The script's output is untouched.
The report goes to stderr when the script ends, even if it crashed or was stopped with Ctrl+C.

Options go **before** the script; everything after the script name is passed to the script:

```bash
hushed run --all job.py --verbose     # --all is for hushed, --verbose is for job.py
```

| Option | What it does |
| --- | --- |
| `-m` | Run a module, like `python -m`. The current directory is watched. |
| `--all` | Also list errors that were caught on purpose (without it they are only counted), and the functions that never ran in every watched file, not only in the script itself. |
| `--exceptions-only` | Skip timing. The script runs at full speed, and only swallowed errors are reported. |
| `--fail-on-swallowed` | Exit with code 3 if the script succeeded but errors were swallowed. Meant for CI. |
| `--json FILE` | Also write the full report to `FILE` as JSON ([format](report.md#json-format)). |
| `--include DIR` | Also watch code under `DIR`. Repeat it for more folders. |
| `--top N` | How many functions the timing table shows (default 10). |

### Which code is watched

hushed reports on your code, not on the libraries it calls. It watches:

- the script's folder (for `hushed run path/to/job.py`),
- the folder itself (for `hushed run path/to/folder`),
- the current directory (for `hushed run -m package.module`),
- and every folder given with `--include`.

The standard library and installed packages (anything in `site-packages` or `dist-packages`) are
never watched. An error raised inside a library still shows up if it reaches your code: hushed then
reports the line in your code that called the library.

### Exit codes

| Code | Meaning |
| --- | --- |
| the script's own | 0, or whatever the script passed to `sys.exit()` |
| 1 | the script crashed with an uncaught exception (Python's normal traceback is printed too) |
| 130 | the script was stopped with Ctrl+C |
| 3 | `--fail-on-swallowed` was given, the script succeeded, and errors were swallowed |
| 2 | hushed itself could not start: bad arguments, no such file, or Python older than 3.12 |

## `hushed analyze`

```text
hushed analyze [OPTIONS] PATH...
```

Reads Python files without running them. A `PATH` can be a file or a folder. Folders are searched
recursively, skipping hidden folders and `__pycache__`, `venv`, `env`, `node_modules`, `build`,
`dist` and `site-packages`.

For each file it lists every function (qualified name, arguments, complexity, whether it returns a
value or yields, whether it has a docstring) and every `except` block that can hide errors.

| Option | What it does |
| --- | --- |
| `-v`, `--verbose` | Also list `print()` calls and assigned variables, per function. |
| `--json FILE` | Also write the result to `FILE` as JSON ([format](report.md#json-format)). |

It exits with 1 if a file could not be read or has a syntax error, and with 0 otherwise.

## `import hushed`

```python
import hushed   # first line of the program's entry point
```

This section is the reference. For examples with their real output, see
[`import hushed`](import-hushed.md).

Importing hushed gives the same report as `hushed run`, without changing how the program is
started: `python app.py`, a launcher script, an IDE's Run button, a scheduled job. From the import
on, the whole program is watched, including every module it imports and every thread it starts.
The report goes to stderr when the program exits, after a crash too (Python's traceback is printed
first, as usual).

Put the import first. Code that ran before it isn't watched, and functions that only ran before it
are listed as never run.

It watches the same code `hushed run` would: the folder of the script Python was started with, or
the current directory for `python -m`. It never changes the program's exit code. Inside
`hushed run` the import does nothing, so the report isn't printed twice. `multiprocessing` workers
and forked children don't print reports of their own.

Its options are environment variables, so they can be changed without touching the code:

| Variable | Same as |
| --- | --- |
| `HUSHED=0` | (no option) Make `import hushed` do nothing, without removing the line. |
| `HUSHED_ALL=1` | `--all` |
| `HUSHED_EXCEPTIONS_ONLY=1` | `--exceptions-only` |
| `HUSHED_JSON=FILE` | `--json FILE` |
| `HUSHED_INCLUDE=DIR` | `--include DIR`; separate several folders with `:` (`;` on Windows) |
| `HUSHED_TOP=N` | `--top N` |

There is no `--fail-on-swallowed`: to fail a CI job, use `hushed run` ([Using hushed in CI](ci.md)),
or write the report with `HUSHED_JSON` and check it afterwards, as
[`examples/check_report.py`](../examples/check_report.py) does.

## Environment variables

| Variable | Effect |
| --- | --- |
| `NO_COLOR` | Never use colours. |
| `FORCE_COLOR` | Use colours even when the output isn't a terminal (for example in CI logs). |
| `TERM=dumb` | Never use colours. |
| `HUSHED`, `HUSHED_*` | Options for [`import hushed`](#import-hushed). |

When the output can't show the report's symbols (for example a legacy Windows console redirected
to a file), hushed falls back to plain ASCII instead of failing.
