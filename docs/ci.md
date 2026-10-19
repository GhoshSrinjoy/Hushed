# Using hushed in CI

Scripts that swallow errors pass in CI too: they exit 0. hushed can turn swallowed errors into a
failing build, and keep a JSON report of what happened.

## Fail a job that swallows errors

```bash
hushed run --fail-on-swallowed nightly_job.py
```

The exit code is the script's own, except that a successful run with swallowed errors exits with 3.
Errors caught on purpose (by a handler for a specific exception type, logged with a traceback, or
suppressed with `contextlib.suppress`) never fail the build. See
[the rules](report.md#when-an-error-counts-as-swallowed).

## GitHub Actions

```yaml
jobs:
  nightly:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: actions/setup-python@v7
        with:
          python-version: "3.13"
      - run: pip install -r requirements.txt hushed
      - run: hushed run --fail-on-swallowed --json hushed-report.json scripts/nightly_job.py
      - uses: actions/upload-artifact@v7
        if: always()                     # keep the report when the job fails
        with:
          name: hushed-report
          path: hushed-report.json
```

Set `FORCE_COLOR: "1"` in the job's `env` to get a coloured report in the log.

## Reading the JSON in a script

```bash
python -c "import json; r = json.load(open('hushed-report.json'))['run']; print(r['summary'])"
```

or with `jq`:

```bash
jq '.run.swallowed[] | select(.hidden) | "\(.type) at \(.raised.file):\(.raised.line): \(.reason)"' hushed-report.json
```

## Flag risky `except` blocks before anything runs

`hushed analyze` finds the `except` blocks that *can* hide errors, without running anything. To fail
a check when there are any:

```bash
hushed analyze --json analysis.json src/
python -c "
import json, sys
files = json.load(open('analysis.json'))['files']
risky = [(f['path'], h['line'], h['clause']) for f in files for h in f['handlers'] if h['hides_errors']]
for path, line, clause in risky:
    print(f'{path}:{line}: {clause}')
sys.exit(1 if risky else 0)
"
```

## Cron jobs and other schedulers

```bash
hushed run --fail-on-swallowed --exceptions-only job.py || notify-team "job.py swallowed errors"
```

`--exceptions-only` skips timing, so the job runs at full speed. The swallowed-error report is
unchanged.
