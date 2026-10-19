"""Turn results into text for people or JSON for tools. No dependencies: colours are plain
ANSI codes, used only when writing to a terminal."""
from __future__ import annotations

import dataclasses
import os
import sys

from . import __version__, static
from .tracer import RunResult, Swallowed

KIND_REASONS = {
    "with": "suppressed by a `with` block",
    "finally": "dropped by a `finally` block (return/break/continue inside it)",
    "thread": "killed its thread; the rest of the program kept going",
    "library": "caught inside library code",
    "unknown": "caught (source not available)",
}
VERDICT_REASONS = {**static.VERDICTS, "reraised": "did not reach its `raise` this time"}
ANSI = {"bold": "1", "dim": "2", "red": "31", "green": "32", "yellow": "33", "cyan": "36",
        "bold red": "1;31", "bold green": "1;32"}


def reason(s: Swallowed) -> str:
    if s.kind == "except" and s.handler is not None:
        return f"`{s.handler.clause}` {VERDICT_REASONS[s.handler.verdict]}"
    return KIND_REASONS.get(s.kind, KIND_REASONS["unknown"])


class Out:
    """Collects lines of (text, style) segments, then prints them in one write."""

    def __init__(self, stream):
        self.stream = stream
        self.lines: list[list[tuple[str, str]]] = []
        try:
            "─✗✓←×·".encode(getattr(stream, "encoding", None) or "ascii")
            self.unicode = True
        except (UnicodeEncodeError, LookupError):
            self.unicode = False

    def sym(self, fancy: str, plain: str) -> str:
        return fancy if self.unicode else plain

    def line(self, *segments) -> None:
        self.lines.append([s if isinstance(s, tuple) else (s, "") for s in segments])

    def rule(self, title: str) -> None:
        bar = self.sym("─", "-")
        self.line((f"{bar * 2} {title} {bar * max(3, 64 - len(title))}", "bold"))

    def flush(self) -> None:
        color = _supports_color(self.stream)
        text = "".join(
            "".join(f"\x1b[{ANSI[style]}m{t}\x1b[0m" if color and style and t else t for t, style in segments)
            + "\n" for segments in self.lines)
        encoding = getattr(self.stream, "encoding", None) or "utf-8"
        try:    # never crash on a character the stream cannot encode (e.g. cp1252 on Windows)
            text = text.encode(encoding, "backslashreplace").decode(encoding, "replace")
        except LookupError:
            text = text.encode("ascii", "backslashreplace").decode("ascii")
        self.stream.write(text)
        self.stream.flush()
        self.lines.clear()


def _supports_color(stream) -> bool:
    if os.environ.get("NO_COLOR") or os.environ.get("TERM") == "dumb":
        return False
    if not getattr(stream, "isatty", lambda: False)():
        return False
    if os.name != "nt":
        return True
    try:    # Windows consoles need virtual-terminal processing switched on for ANSI codes
        import ctypes
        import msvcrt
        from ctypes import wintypes

        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.GetConsoleMode.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
        kernel32.SetConsoleMode.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        handle = msvcrt.get_osfhandle(stream.fileno())
        mode = wintypes.DWORD()
        if not kernel32.GetConsoleMode(handle, ctypes.byref(mode)):
            return False
        return bool(mode.value & 0x0004 or kernel32.SetConsoleMode(handle, mode.value | 0x0004))
    except (OSError, AttributeError, ValueError, ImportError):
        return False


# ---------------------------------------------------------------- run report

def print_run(result: RunResult, stream=None, show_all: bool = False, top: int = 10) -> None:
    out = Out(stream or sys.stderr)
    out.line()
    out.rule(f"whathappened: {result.target}")
    out.line(*_status(result), (f"  ({result.engine} engine)", "dim"))

    hidden = [s for s in result.swallowed if s.hidden]
    handled = [s for s in result.swallowed if not s.hidden]
    looked_fine = result.status == "ok" or (result.status == "exit" and result.exit_code == 0)
    if hidden:
        n = sum(s.count for s in hidden)
        headline = f"{out.sym('✗', 'x')} {n} error{'s were' if n > 1 else ' was'} swallowed"
        out.line()
        out.line((headline, "bold red"), (" (the run looked fine)" if looked_fine else "", "red"))
        for s in hidden:
            _swallowed(out, s)
    else:
        out.line()
        out.line((f"{out.sym('✓', '+')} no swallowed errors", "bold green"))
    if handled and show_all:
        out.line()
        out.line((f"Caught on purpose ({sum(s.count for s in handled)})", "bold"))
        for s in handled:
            _swallowed(out, s)
    elif handled:
        n = sum(s.count for s in handled)
        out.line((f"  + {n} more caught on purpose (fallback, logged or suppressed); "
                  f"show them with --all", "dim"))

    if result.timing and result.functions:
        out.line()
        out.line(("Time per function", "bold"), ("  (includes the calls it makes)", "dim"))
        shown = result.functions[:top]
        width = min(48, max(len(f.function) for f in shown))
        for f in shown:
            calls = f"{f.calls} call" + ("s" if f.calls != 1 else "")
            out.line(f"  {f.seconds:9.3f}s  ", (f"{f.function:<{width}}", "cyan"), f"  {calls}")
        if len(result.functions) > top:
            out.line((f"  ... {len(result.functions) - top} more", "dim"))
    if result.timing and result.never_ran:
        names = [f"{loc.function} ({loc.file}:{loc.line})" for loc in result.never_ran[:10]]
        more = len(result.never_ran) - len(names)
        out.line()
        out.line((f"Never ran ({len(result.never_ran)}): ", "bold"),
                 ", ".join(names) + (f" and {more} more" if more > 0 else ""))
    out.line()
    out.flush()


def _status(result: RunResult) -> list:
    took = f" in {result.seconds:.2f}s"
    if result.status == "ok":
        return [("finished normally", "green"), took]
    if result.status == "exit":
        style = "green" if result.exit_code == 0 else "yellow"
        return [(f"exited with code {result.exit_code}", style), took]
    if result.status == "interrupted":
        return [("interrupted", "yellow"), f" after {result.seconds:.2f}s"]
    where = ""
    if result.error_at is not None:
        at = result.error_at
        where = f" at {at.function} ({at.file}:{at.line})"
    return [("crashed", "bold red"), took, f": {result.error}{where}"]


def _swallowed(out: Out, s: Swallowed) -> None:
    title = f"{s.type}: {s.message}" if s.message else s.type
    times = f"  {out.sym('×', 'x')}{s.count}" if s.count > 1 else ""
    out.line("  ", (title, "bold"), (times, "yellow"))
    width = max(len(s.raised.function), len(s.caught.function))
    out.line(("      raised  ", "dim"), f"{s.raised.function:<{width}}  {s.raised.file}:{s.raised.line}")
    out.line(("      caught  ", "dim"), f"{s.caught.function:<{width}}  {s.caught.file}:{s.caught.line}",
             (f"  {out.sym('←', '<-')} {reason(s)}", "yellow" if s.hidden else "dim"))


# ---------------------------------------------------------------- static report

def print_analysis(reports: list[static.FileReport], stream=None, verbose: bool = False) -> None:
    out = Out(stream or sys.stdout)
    for r in reports:
        out.line()
        if r.error:
            out.rule(r.path)
            out.line(("  could not analyze: ", "red"), r.error)
            continue
        hiding = [h for h in r.handlers if h.hides_errors]
        dot = out.sym("·", "-")
        out.rule(f"{r.path} {dot} {len(r.functions)} functions {dot} {len(r.handlers)} except blocks "
                 f"({len(hiding)} can hide errors) {dot} {len(r.prints)} print() calls")
        if r.functions:
            _functions_table(out, r.functions)
        if hiding:
            out.line()
            out.line(("  except blocks that can hide errors", "bold"))
            width = max(len(h.scope) for h in hiding)
            for h in hiding:
                out.line(f"  {h.line:>5}  ", (f"{h.scope:<{width}}", "cyan"),
                         f"  `{h.clause}` ", (static.VERDICTS[h.verdict], "yellow"))
        if verbose:
            _by_scope(out, "print() calls", _group(r.prints))
            _by_scope(out, "assigned variables", r.variables)
    out.line()
    out.flush()


def _functions_table(out: Out, functions: list[static.Function]) -> None:
    rows = [(str(f.line), ("async " if f.is_async else "") + f.qualname, _clip(", ".join(f.args) or "-", 32),
             str(f.complexity), f.returns, "yes" if f.docstring else "no") for f in functions]
    head = ("line", "function", "args", "complexity", "returns", "docstring")
    widths = [max(len(head[i]), *(len(row[i]) for row in rows)) for i in range(len(head))]
    out.line()
    out.line(("  " + _cells(head, widths), "dim"))
    for row in rows:
        style = "yellow" if int(row[3]) > 10 else ""
        out.line("  ", (_cells(row, widths), style))


def _cells(row, widths) -> str:
    right = {0, 3}       # numeric columns
    return "  ".join(cell.rjust(w) if i in right else cell.ljust(w)
                     for i, (cell, w) in enumerate(zip(row, widths))).rstrip()


def _clip(text: str, width: int) -> str:
    return text if len(text) <= width else text[: width - 3] + "..."


def _group(prints: list[tuple[int, str]]) -> dict[str, list[str]]:
    grouped: dict[str, list[str]] = {}
    for line, scope in prints:
        grouped.setdefault(scope, []).append(str(line))
    return grouped


def _by_scope(out: Out, title: str, items: dict[str, list[str]]) -> None:
    if not items:
        return
    out.line()
    out.line((f"  {title}", "bold"))
    for scope, values in items.items():
        out.line("    ", (scope, "cyan"), ": " + ", ".join(values))


# ---------------------------------------------------------------- JSON

def run_json(result: RunResult) -> dict:
    data = dataclasses.asdict(result)
    for item, s in zip(data["swallowed"], result.swallowed):
        item["reason"] = reason(s)
    data["summary"] = {
        "hidden_errors": sum(s.count for s in result.swallowed if s.hidden),
        "caught_on_purpose": sum(s.count for s in result.swallowed if not s.hidden),
    }
    return {"tool": "whathappened", "version": __version__, "run": data}


def analysis_json(reports: list[static.FileReport]) -> dict:
    files = []
    for r in reports:
        data = dataclasses.asdict(r)
        for item, h in zip(data["handlers"], r.handlers):
            item["clause"] = h.clause
            item["hides_errors"] = h.hides_errors
        files.append(data)
    return {"tool": "whathappened", "version": __version__, "files": files}
