# Commands

`hushed` has two commands. `python -m hushed ...` works the same as `hushed ...`.

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
| `--all` | Also list errors that were caught on purpose. Without it they are only counted. |
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

## Environment variables

| Variable | Effect |
| --- | --- |
| `NO_COLOR` | Never use colours. |
| `FORCE_COLOR` | Use colours even when the output isn't a terminal (for example in CI logs). |
| `TERM=dumb` | Never use colours. |

When the output can't show the report's symbols (for example a legacy Windows console redirected
to a file), hushed falls back to plain ASCII instead of failing.
