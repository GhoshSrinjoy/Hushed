# Examples

| Example | Shows |
| --- | --- |
| [`pipeline.py`](pipeline.py) | `hushed run`: a data job prints "done" while part of its work failed |
| [`app/`](app/main.py) | `import hushed`: one line at the top of an app's entry point watches every module and thread |
| [`check_report.py`](check_report.py) | reading the JSON report from Python, for example to fail a CI job |

Run them from the repository root:

```bash
hushed run examples/pipeline.py        # watch a script without changing it
python examples/app/main.py            # the app imports hushed itself; the report prints when it exits
```

`import hushed` takes its options from environment variables ([all of them](../docs/commands.md#import-hushed)):

```bash
HUSHED_ALL=1 python examples/app/main.py              # also list the errors caught on purpose
HUSHED_JSON=report.json python examples/app/main.py   # also write the report as JSON
python examples/check_report.py report.json           # lists the swallowed errors; exit code 1
HUSHED=0 python examples/app/main.py                  # off, without removing the import
```

In PowerShell, set the variable first: `$env:HUSHED_JSON = "report.json"; python examples/app/main.py`.
