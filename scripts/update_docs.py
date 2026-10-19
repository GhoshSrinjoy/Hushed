"""Fill the code and output blocks in the docs from the real files and real runs.

    python scripts/update_docs.py

In README.md, examples/README.md and docs/*.md, a marker comment fills the fenced block right after it:

- `<!-- file: PATH -->`: the block becomes the contents of PATH.
- `<!-- output: COMMAND -->`: COMMAND runs, and the block becomes `$ COMMAND` and everything it
  printed, stdout and stderr in order.
- `<!-- run: COMMAND -->`: COMMAND runs and nothing is shown, for example to write a file that the
  next command reads.

Commands look like `python examples/quickstart.py`, `hushed run examples/pipeline.py` or
`HUSHED_ALL=1 python ...`. They run from a copy of examples/, so files they write stay out of the
repository. Needs hushed built in place (`maturin develop`) or installed. Times change from run to run.
"""
import contextlib
import glob
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOCS = ["README.md", os.path.join("examples", "README.md")] + sorted(
    os.path.relpath(p, ROOT) for p in glob.glob(os.path.join(ROOT, "docs", "*.md")))
MARKER = re.compile(r"<!-- (file|output|run): (.+?) -->\n(?:(```[a-z]*\n).*?```\n)?", re.S)
PROGRAMS = {"python": [sys.executable], "hushed": [sys.executable, "-m", "hushed"]}


def run(command, work):
    words = shlex.split(command)
    env = {**os.environ, "NO_COLOR": "1", "PYTHONIOENCODING": "utf-8", "PYTHONUNBUFFERED": "1"}
    env.pop("FORCE_COLOR", None)
    if glob.glob(os.path.join(ROOT, "hushed", "_native*")):  # built in place: use the checkout
        env["PYTHONPATH"] = os.pathsep.join(filter(None, [ROOT, os.environ.get("PYTHONPATH")]))
    while "=" in words[0]:
        name, value = words.pop(0).split("=", 1)
        env[name] = value
    proc = subprocess.run(PROGRAMS[words[0]] + words[1:], cwd=work, env=env, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, text=True, encoding="utf-8")
    output = proc.stdout
    # tracebacks name the temporary copy, perhaps by its real path (/private/var/... on macOS):
    # take the longer name out first, or the shorter one would match inside it
    for folder in sorted({work, os.path.realpath(work)}, key=len, reverse=True):
        output = output.replace(folder + os.sep, "")
    return re.sub(r"examples\\[\w\\.]+", lambda m: m.group(0).replace("\\", "/"), output)


def fill(text, work):
    def block(m):
        kind, arg, fence = m.groups()
        if kind == "run":
            run(arg, work)
            return m.group(0)
        if kind == "file":
            with open(os.path.join(ROOT, arg), encoding="utf-8") as f:
                body = f.read()
            return f"<!-- file: {arg} -->\n{fence or '```python' + chr(10)}{body}```\n"
        return f"<!-- output: {arg} -->\n```text\n$ {arg}\n{run(arg, work).rstrip()}\n```\n"
    return MARKER.sub(block, text)


@contextlib.contextmanager
def workspace():
    """A temporary folder with a copy of examples/, to run the commands in."""
    with tempfile.TemporaryDirectory() as work:
        shutil.copytree(os.path.join(ROOT, "examples"), os.path.join(work, "examples"),
                        ignore=shutil.ignore_patterns("__pycache__"))
        yield work


def main():
    with workspace() as work:
        for doc in DOCS:
            path = os.path.join(ROOT, doc)
            with open(path, encoding="utf-8") as f:
                text = f.read()
            new = fill(text, work)
            if new != text:
                with open(path, "w", encoding="utf-8", newline="\n") as f:
                    f.write(new)
                print("updated", doc)


if __name__ == "__main__":
    main()
