# Development

## Set up

You need Rust (<https://rustup.rs>) and Python 3.12 or newer.

```bash
python -m venv .venv
source .venv/bin/activate              # Windows: .venv\Scripts\activate
pip install maturin
maturin develop --release              # compile the Rust part into the package
```

Run `maturin develop --release` again after changing Rust code.

## Test and lint

```bash
python -m unittest discover -s tests   # end-to-end tests that drive the real command
cargo fmt --check                      # formatting
cargo clippy --release -- -D warnings  # lints
```

The tests run `python -m hushed` from an empty folder. If the Rust part was built into the checkout,
they use the checkout; otherwise they use the installed package. CI runs them on Linux, macOS and
Windows with Python 3.12, 3.13 and 3.14 (see `.github/workflows/ci.yml`).

## Screenshots and example output

The images and the example code and output in the README and docs come from real files and runs:

```bash
python scripts/make_screenshots.py     # writes docs/images/*.svg
python scripts/update_docs.py          # fills the <!-- file: --> and <!-- output: --> blocks
```

Run both after changing the report or the examples. Times change from run to run.

## Layout

| Path | What it holds |
| --- | --- |
| `src/cli.rs` | the command line |
| `src/auto.rs` | `import hushed`: watching from the import, the report at exit |
| `src/tracer.rs` | runs the script under `sys.monitoring` and builds the report data |
| `src/monitor.rs` | the hooks that run on every call, return and exception |
| `src/source.rs` | source analysis: functions, complexity, what each `except` block does |
| `src/report.rs` | text and JSON output |
| `src/paths.rs` | which files count as the user's code |
| `hushed/` | the Python package: just the launcher, and the call that starts `import hushed` |
| `tests/` | end-to-end tests and their fixtures |
| `examples/` | example scripts used in the docs (`examples/README.md` lists them) |
| `scripts/` | maintenance scripts |

## Releasing

Releases are built and published by `.github/workflows/release.yml` when a version tag is pushed.
It builds one wheel per platform:

- Linux: x86_64 and aarch64, for both glibc and musl
- macOS: x86_64 and arm64
- Windows: x64

The wheels use Python's stable ABI, so each one covers every Python from 3.12 on. The workflow
also builds a source distribution, creates a GitHub release with all files, and uploads them to
PyPI.

1. Set the new version in `Cargo.toml` (the only place it lives) and add a `CHANGELOG.md` entry.
2. Commit, then tag and push:

   ```bash
   git tag v0.3.0
   git push origin v0.3.0
   ```

The workflow refuses to publish if the tag doesn't match the version in `Cargo.toml`. To try the
whole build without publishing, run the workflow by hand from the Actions tab.

### One-time PyPI setup

The workflow uploads with PyPI's trusted publishing, so no API token is stored anywhere. Before the
first release:

1. Sign in at <https://pypi.org>, open **Your account → Publishing**, and add a pending publisher:
   - PyPI project name: `hushed`
   - Owner: `GhoshSrinjoy`
   - Repository: `Hushed`
   - Workflow: `release.yml`
   - Environment: `pypi`
2. Optional: in the GitHub repository, open **Settings → Environments → New environment**, name it
   `pypi`, and add yourself as a required reviewer. Each upload then waits for your approval.
