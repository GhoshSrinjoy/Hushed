"""Runtime side: run a script under sys.monitoring (PEP 669) and record what actually happened.

Nothing in the target is rewritten or wrapped, and no reference to any exception is kept
(only its id while it is in flight), so frames, open files and locals are freed exactly
when they would be without the tracer.
"""
from __future__ import annotations

import os
import runpy
import sys
import sysconfig
import threading
import time
from dataclasses import dataclass, field

from . import static

try:
    from . import _native       # the Rust engine (src/lib.rs); optional
except ImportError:
    _native = None

mon = sys.monitoring
E = mon.events
CONTROL_FLOW = (StopIteration, StopAsyncIteration, GeneratorExit)
MAX_PENDING = 10_000
PACKAGE_DIR = os.path.normcase(os.path.dirname(os.path.abspath(__file__))) + os.sep
RUNPY_FILE = os.path.normcase(runpy.__file__)


@dataclass(frozen=True)
class Location:
    file: str
    line: int
    function: str


@dataclass
class Swallowed:
    """Exceptions with the same type, raise point and catch point, after which the run went on."""
    type: str
    message: str            # of the first occurrence
    count: int
    raised: Location
    caught: Location
    kind: str               # "except", "with", "finally", "thread", "library" or "unknown"
    hidden: bool            # True: the error vanished; False: caught on purpose and handled visibly
    handler: static.Handler | None = None


@dataclass
class FunctionStats:
    function: str
    file: str
    line: int
    calls: int
    seconds: float          # inclusive; recursive calls are not double counted


@dataclass
class RunResult:
    target: str
    status: str             # "ok", "exit", "crash" or "interrupted"
    exit_code: int
    seconds: float
    timing: bool
    engine: str             # "rust" or "python"
    error: str | None = None
    error_at: Location | None = None
    swallowed: list[Swallowed] = field(default_factory=list)
    functions: list[FunctionStats] = field(default_factory=list)
    never_ran: list[Location] = field(default_factory=list)
    files: list[str] = field(default_factory=list)


class _Pending:
    """An exception being followed. Holds its class and code objects, never the exception."""
    __slots__ = ("type", "origin", "raised", "caught", "message")

    def __init__(self, exc_type, origin, raised):
        self.type = exc_type
        self.origin = origin      # code of the innermost traceback frame: tells ids apart
        self.raised = raised      # (code, offset)
        self.caught = None        # (code, offset) while it sits in a handler
        self.message = ""


def engines() -> list[str]:
    return ["rust", "python"] if _native is not None else ["python"]


class Tracer:
    def __init__(self, roots: list[str], timing: bool = True, engine: str = "auto"):
        if engine == "auto":
            engine = engines()[0]
        if engine not in engines():
            raise ValueError(f"the {engine} engine is not available (built engines: {', '.join(engines())})")
        self.engine = engine
        self._monitor = None
        self.roots = tuple(os.path.join(os.path.normcase(os.path.abspath(r)), "") for r in roots)
        paths = sysconfig.get_paths()
        self.skip = tuple({os.path.join(os.path.normcase(os.path.abspath(paths[k])), "")
                           for k in ("stdlib", "platstdlib", "purelib", "platlib")} | {PACKAGE_DIR})
        self.timing = timing
        self.mine: dict = {}          # code -> is it the user's code?
        self.calls: dict = {}         # code -> number of calls
        self.spent: dict = {}         # code -> seconds in outermost activations
        self.threads: dict = {}       # thread id -> (start times, recursion depth per code)
        self.pending: dict = {}       # id(exception) -> _Pending
        self.settled: dict = {}       # (type, raised, caught) -> [count, first message]
        self._lines: dict = {}

    # ------------------------------------------------------------ running

    def run(self, target: str, args: list[str], module: bool = False) -> RunResult:
        tool = next((i for i in (3, 4) if mon.get_tool(i) is None), None)   # ids with no assigned role
        if tool is None:
            raise RuntimeError("no free sys.monitoring tool id (another tool is using 3 and 4)")
        callbacks = self._native_callbacks() if self.engine == "rust" else self._callbacks()
        mon.use_tool_id(tool, "whathappened")
        for event, callback in callbacks.items():
            mon.register_callback(tool, event, callback)

        saved_argv, saved_path0 = sys.argv[:], sys.path[0]
        sys.argv = [target, *args]
        if module:
            sys.path[0] = os.getcwd()
        else:   # like `python script.py` (or `python folder/`, which runs folder/__main__.py)
            path = os.path.abspath(target)
            sys.path[0] = path if os.path.isdir(path) else os.path.dirname(path)
        status, exit_code, crash = "ok", 0, None
        start = time.perf_counter()
        mon.set_events(tool, sum(callbacks))
        try:
            if module:
                runpy.run_module(target, run_name="__main__", alter_sys=True)
            else:
                runpy.run_path(os.path.abspath(target), run_name="__main__")
        except SystemExit as e:
            status, exit_code = "exit", _exit_code(e.code)
        except KeyboardInterrupt as e:
            status, exit_code, crash = "interrupted", 130, e
        except BaseException as e:
            status, exit_code, crash = "crash", 1, e
        finally:
            mon.set_events(tool, 0)
            seconds = time.perf_counter() - start
            for event in callbacks:
                mon.register_callback(tool, event, None)
            mon.free_tool_id(tool)
            sys.argv, sys.path[0] = saved_argv, saved_path0

        result = RunResult(target=target, status=status, exit_code=exit_code,
                           seconds=seconds, timing=self.timing, engine=self.engine)
        if crash is not None:
            tb = _user_traceback(crash.__traceback__)
            crash.__traceback__ = tb                    # 3.12's hook ignores `tb` when one is attached
            sys.excepthook(type(crash), crash, tb)      # print it the way Python would
            message = _message(crash)
            result.error = f"{type(crash).__name__}: {message}" if message else type(crash).__name__
            result.error_at = self._crash_location(tb)
            crash = tb = None
        if self._monitor is not None:
            self._collect_native()
        for key in list(self.pending):
            self._settle(key)
        self._fill(result)
        return result

    def _native_callbacks(self) -> dict:
        m = self._monitor = _native.Monitor(self._is_mine, _message, mon.DISABLE,
                                            Tracer.run.__code__, CONTROL_FLOW)
        hooks = {E.RAISE: m.on_raise, E.EXCEPTION_HANDLED: m.on_handled, E.RERAISE: m.on_reraise}
        if self.timing:
            hooks.update({E.PY_START: m.on_start, E.PY_RESUME: m.on_resume, E.PY_THROW: m.on_throw,
                          E.PY_RETURN: m.on_leave, E.PY_YIELD: m.on_leave, E.PY_UNWIND: m.on_unwind})
        return hooks

    def _collect_native(self) -> None:
        """Copy what the Rust engine recorded into the same tables the Python engine fills."""
        self._monitor.settle_all()
        data = self._monitor.results()
        self._monitor = None
        self.mine.update(data["mine"])
        self.calls.update(data["calls"])
        self.spent.update(data["spent"])
        for exc_type, raised_code, raised_at, caught_code, caught_at, count, message in data["settled"]:
            self.settled[(exc_type, (raised_code, raised_at), (caught_code, caught_at))] = [count, message]

    def _callbacks(self) -> dict:
        mine, calls, spent, threads, pending = self.mine, self.calls, self.spent, self.threads, self.pending
        classify, settle = self._is_mine, self._settle
        perf, get_ident, DISABLE = time.perf_counter, threading.get_ident, mon.DISABLE
        own_code = Tracer.run.__code__

        def is_mine(code):
            m = mine.get(code)
            if m is None:
                m = mine[code] = classify(code)
            return m

        def frames():
            t = threads.get(get_ident())
            if t is None:
                t = threads[get_ident()] = ([], {})
            return t

        def enter(code):
            stack, depth = frames()
            depth[code] = depth.get(code, 0) + 1
            stack.append(perf())

        def leave(code):
            stack, depth = frames()
            if stack:
                started = stack.pop()
                d = depth[code] = depth.get(code, 1) - 1
                if d <= 0:
                    spent[code] = spent.get(code, 0.0) + perf() - started

        def on_start(code, offset):
            if not is_mine(code):
                return DISABLE
            calls[code] = calls.get(code, 0) + 1
            enter(code)

        def on_resume(code, offset):
            if not is_mine(code):
                return DISABLE
            enter(code)

        def on_leave(code, offset, value):      # PY_RETURN and PY_YIELD
            if not is_mine(code):
                return DISABLE
            leave(code)

        def on_throw(code, offset, exc):        # cannot be disabled
            if is_mine(code):
                enter(code)

        def on_unwind(code, offset, exc):       # cannot be disabled
            if is_mine(code):
                leave(code)

        def innermost(tb):
            while tb.tb_next is not None:
                tb = tb.tb_next
            return tb.tb_frame.f_code

        def on_raise(code, offset, exc):
            key, tb = id(exc), exc.__traceback__
            rec = pending.get(key)
            if rec is not None:
                if tb is not None and tb.tb_next is not None and innermost(tb) is rec.origin:
                    rec.caught = None           # same exception, propagating or raised again
                    return
                settle(key)                     # an earlier exception that had the same id
            if isinstance(exc, CONTROL_FLOW) or not is_mine(code):
                return
            pending[key] = _Pending(type(exc), innermost(tb) if tb is not None else code, (code, offset))
            if len(pending) > MAX_PENDING:
                for old in list(pending)[: MAX_PENDING // 2]:
                    settle(old)
            ctx = exc.__context__
            if ctx is not None and id(ctx) in pending:
                pending[id(ctx)].caught = None  # chained into the new exception, so not lost

        def on_handled(code, offset, exc):
            if code is own_code:
                return                          # our own `except` around the run
            key = id(exc)
            rec = pending.get(key)
            if rec is None:
                if isinstance(exc, CONTROL_FLOW) or not is_mine(code):
                    return
                tb = exc.__traceback__
                rec = pending[key] = _Pending(type(exc), innermost(tb) if tb is not None else code,
                                              (code, offset))
            rec.caught = (code, offset)
            if not rec.message:
                rec.message = _message(exc)

        def on_reraise(code, offset, exc):
            rec = pending.get(id(exc))
            if rec is not None:
                rec.caught = None

        hooks = {E.RAISE: on_raise, E.EXCEPTION_HANDLED: on_handled, E.RERAISE: on_reraise}
        if self.timing:
            hooks.update({E.PY_START: on_start, E.PY_RESUME: on_resume, E.PY_THROW: on_throw,
                          E.PY_RETURN: on_leave, E.PY_YIELD: on_leave, E.PY_UNWIND: on_unwind})
        return hooks

    def _is_mine(self, code) -> bool:
        path = os.path.normcase(code.co_filename)
        return (path.startswith(self.roots) and not path.startswith(self.skip)
                and "site-packages" not in path and "dist-packages" not in path)

    def _settle(self, key) -> None:
        rec = self.pending.pop(key)
        if rec.caught is None:
            return
        entry = self.settled.get((rec.type, rec.raised, rec.caught))
        if entry is None:
            self.settled[(rec.type, rec.raised, rec.caught)] = [1, rec.message]
        else:
            entry[0] += 1

    # ------------------------------------------------------------ results

    def _fill(self, result: RunResult) -> None:
        for (exc_type, raised, caught), (count, message) in self.settled.items():
            result.swallowed.append(self._swallowed(exc_type, raised, caught, count, message))
        result.swallowed.sort(key=lambda s: (not s.hidden, -s.count, s.raised.file, s.raised.line))

        executed = {(os.path.normcase(c.co_filename), c.co_qualname, c.co_firstlineno) for c in self.calls}
        files = sorted({c.co_filename for c, mine in self.mine.items() if mine})
        result.files = [self._display(f) for f in files]
        if not self.timing:
            return
        result.functions = sorted(
            (FunctionStats(c.co_qualname, self._display(c.co_filename), c.co_firstlineno,
                           n, self.spent.get(c, 0.0))
             for c, n in self.calls.items() if c.co_name != "<module>"),
            key=lambda f: -f.seconds)
        for path in files:
            for fn in static.defined_functions(path):
                if any(d.endswith(("overload", "abstractmethod")) for d in fn.decorators):
                    continue
                if not any((os.path.normcase(path), fn.qualname, ln) in executed
                           for ln in (fn.line, fn.first_line)):
                    result.never_ran.append(Location(self._display(path), fn.line, fn.qualname))

    def _swallowed(self, exc_type, raised, caught, count, message) -> Swallowed:
        code = caught[0]
        caught_at = self._location(*caught)
        s = Swallowed(type=exc_type.__name__, message=message, count=count,
                      raised=self._location(*raised), caught=caught_at, kind="unknown", hidden=False)
        if not self._is_mine(code):     # (with --exceptions-only, not every code object was classified)
            s.kind = "thread" if os.path.basename(code.co_filename) == "threading.py" else "library"
            s.hidden = True
            return s
        names = frozenset(c.__name__ for c in exc_type.__mro__)
        site = static.catch_site(code.co_filename, caught_at.line, names)
        s.kind, s.handler = site.kind, site.handler
        if site.kind == "finally":
            s.hidden = True                     # return/break/continue inside `finally` dropped it
        elif site.handler is not None:
            h = site.handler
            s.caught = Location(caught_at.file, h.line, caught_at.function)   # the clause that matched
            s.hidden =h.verdict in static.HIDING_VERDICTS or (h.verdict in ("fallback", "reraised") and h.broad)
        return s

    def _location(self, code, offset) -> Location:
        return Location(self._display(code.co_filename), self._line(code, offset), code.co_qualname)

    def _line(self, code, offset) -> int:
        table = self._lines.get(code)
        if table is None:
            table = self._lines[code] = list(code.co_lines())
        for _start, end, line in table:
            if end > offset and line is not None:
                return line
        return code.co_firstlineno

    def _display(self, filename: str) -> str:
        path = os.path.normcase(os.path.abspath(filename))
        for root in self.roots + self.skip:
            if path.startswith(root):
                return os.path.relpath(os.path.abspath(filename), root[:-1]).replace(os.sep, "/")
        return os.path.basename(filename)

    def _crash_location(self, tb) -> Location | None:
        found = None
        while tb is not None:
            code = tb.tb_frame.f_code
            if self._is_mine(code):
                found = Location(self._display(code.co_filename), tb.tb_lineno, code.co_qualname)
            tb = tb.tb_next
        return found


def _user_traceback(tb):
    """Drop the frames of whathappened and runpy from the top of a traceback."""
    while tb is not None:
        f = os.path.normcase(tb.tb_frame.f_code.co_filename)
        if not (f.startswith(PACKAGE_DIR) or f == RUNPY_FILE or f.startswith("<frozen")):
            break
        tb = tb.tb_next
    return tb


def _exit_code(code) -> int:
    """Python's own rules for turning a SystemExit code into a process exit status."""
    if code is None:
        return 0
    if isinstance(code, int):
        return code
    print(code, file=sys.stderr)
    return 1


def _message(exc) -> str:
    try:
        text = " ".join(str(exc).split())
    except Exception:
        return "<unprintable>"
    return text if len(text) <= 160 else text[:157] + "..."
