# hushed documentation

`hushed` runs a Python script unchanged and tells you what actually happened, including every error
that was raised and then swallowed.

| Page | What it covers |
| --- | --- |
| [Getting started](getting-started.md) | Install, your first run, and how to read the report |
| [`import hushed`](import-hushed.md) | Using hushed from inside your program: examples with their real output, every option |
| [Commands](commands.md) | Every option of `hushed run` and `hushed analyze`, exit codes, environment variables |
| [The report](report.md) | What each part of the report means, how `except` blocks are judged, the JSON format |
| [Using hushed in CI](ci.md) | Failing a build when errors are swallowed, keeping JSON reports |
| [How it works](how-it-works.md) | `sys.monitoring`, the Rust engine, overhead, limitations |
| [Development](development.md) | Building, testing and releasing hushed itself |
