# Examples

| Example | Shows |
| --- | --- |
| [`quickstart.py`](quickstart.py) | `import hushed`: one line at the top, and the report when the program exits |
| [`app/`](app/main.py) | `import hushed` in an app with several modules and a thread pool |
| [`crash.py`](crash.py) | a crash whose real cause was swallowed earlier |
| [`pipeline.py`](pipeline.py) | `hushed run`: watching a script without changing it |
| [`check_report.py`](check_report.py) | reading the JSON report from Python, for example to fail a CI job |

Run them from the repository root. Each output below is exactly what the example prints (made by
`python scripts/update_docs.py`). The code, with explanations and every option, is in
[`import hushed`](../docs/import-hushed.md).

## quickstart.py

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

## app/

<!-- output: python examples/app/main.py -->
```text
$ python examples/app/main.py
synced 3 users

── hushed: examples/app/main.py ────────────────────────────────────
finished normally in 0.02s

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

## crash.py

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

## pipeline.py

<!-- output: hushed run examples/pipeline.py -->
```text
$ hushed run examples/pipeline.py
warning: could not convert string to float: 'N/A'
done: 3 records, total 14.49 USD

── hushed: examples/pipeline.py ────────────────────────────────────
finished normally in 0.01s

✗ 3 errors were swallowed (the run looked fine)
  FileNotFoundError: [Errno 2] No such file or directory: 'settings.json'
      raised  load_settings  pipeline.py:17
      caught  load_settings  pipeline.py:19  ← `except Exception` took a fallback path
  JSONDecodeError: Expecting property name enclosed in double quotes: line 1 column 2 (char 1)
      raised  parse  pipeline.py:25
      caught  parse  pipeline.py:26  ← `except Exception` silently ignored
  ValueError: could not convert string to float: 'N/A'
      raised  price  pipeline.py:32
      caught  price  pipeline.py:33  ← `except Exception` only printed the message; traceback lost

Time per function  (includes the calls it makes)
      0.000s  main           1 call
      0.000s  load_settings  1 call
      0.000s  parse          4 calls
      0.000s  price          3 calls
```

## check_report.py

Write the report as JSON, then read it from Python:

<!-- run: HUSHED_JSON=report.json python examples/app/main.py -->
```bash
HUSHED_JSON=report.json python examples/app/main.py
```

<!-- output: python examples/check_report.py report.json -->
```text
$ python examples/check_report.py report.json
remote.py:14: AttributeError: 'NoneType' object has no attribute 'split'  (x1, `except Exception` silently ignored)
remote.py:14: ValueError: invalid email for user 2: grace@example  (x1, `except Exception` silently ignored)
users.py:15: FileNotFoundError: [Errno 2] No such file or directory: 'users.json'  (x1, `except Exception` took a fallback path)
3 swallowed error(s)
```

In PowerShell, set a variable first: `$env:HUSHED_JSON = "report.json"; python examples/app/main.py`.
