"""Static side: read Python source and describe its functions and `except` blocks."""
from __future__ import annotations

import ast
import tokenize
from dataclasses import dataclass, field
from functools import lru_cache

SCOPES = (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef, ast.Lambda)
TRY_NODES = (ast.Try, ast.TryStar)
BROAD = {"Exception", "BaseException"}
TRACEBACK_CALLS = {"exception", "print_exc", "format_exc", "print_exception",
                   "format_exception", "capture_exception"}
LOG_CALLS = {"error", "warning", "warn", "info", "debug", "critical", "fatal", "log"}

VERDICTS = {
    "ignored": "silently ignored",
    "printed": "only printed the message; traceback lost",
    "logged-message": "logged the message only; traceback lost",
    "logged": "logged with traceback",
    "fallback": "took a fallback path",
    "reraised": "re-raised",
}
HIDING_VERDICTS = {"ignored", "printed", "logged-message"}


@dataclass
class Handler:
    """One `except` clause."""
    line: int
    scope: str          # qualified name of the enclosing function, or "<module>"
    catches: str        # source text of the caught types; "" for a bare `except:`
    broad: bool         # bare, `Exception` or `BaseException`
    verdict: str        # key of VERDICTS

    @property
    def clause(self) -> str:
        return f"except {self.catches}" if self.catches else "except:"

    @property
    def hides_errors(self) -> bool:
        return self.verdict in HIDING_VERDICTS or (self.verdict == "fallback" and self.broad)


@dataclass
class Function:
    qualname: str
    line: int           # the `def` line
    first_line: int     # first decorator line, which is what code.co_firstlineno reports
    is_async: bool
    args: list[str]
    complexity: int     # McCabe cyclomatic complexity of this function's own body
    returns: str        # "value", "yields" or "none"
    docstring: bool
    decorators: list[str] = field(default_factory=list)


@dataclass
class FileReport:
    path: str
    functions: list[Function] = field(default_factory=list)
    handlers: list[Handler] = field(default_factory=list)
    prints: list[tuple[int, str]] = field(default_factory=list)       # (line, scope)
    variables: dict[str, list[str]] = field(default_factory=dict)    # scope -> assigned names
    error: str | None = None


@lru_cache(maxsize=None)
def parse_file(path: str) -> ast.Module | None:
    """Parse a source file (honouring its encoding declaration); None if unreadable."""
    try:
        with tokenize.open(path) as f:
            return ast.parse(f.read(), filename=path)
    except (OSError, SyntaxError, ValueError, UnicodeDecodeError):
        return None


def analyze_file(path: str) -> FileReport:
    try:
        with tokenize.open(path) as f:
            tree = ast.parse(f.read(), filename=path)
    except SyntaxError as e:
        return FileReport(path, error=f"syntax error at line {e.lineno}: {e.msg}")
    except (OSError, ValueError, UnicodeDecodeError) as e:
        return FileReport(path, error=str(e))
    collector = _Collector()
    collector.visit(tree)
    return FileReport(
        path=path,
        functions=collector.functions,
        handlers=collector.handlers,
        prints=collector.prints,
        variables={scope: sorted(names) for scope, names in collector.variables.items()},
    )


def defined_functions(path: str) -> list[Function]:
    tree = parse_file(path)
    if tree is None:
        return []
    collector = _Collector()
    collector.visit(tree)
    return collector.functions


@dataclass
class CatchSite:
    """Where a runtime exception was caught, read back from the source."""
    kind: str                   # "except", "with", "finally" or "unknown"
    handler: Handler | None = None


def catch_site(path: str, line: int, type_names: frozenset[str]) -> CatchSite:
    """Find the construct that caught an exception at `line`.

    Python reports the first clause of a try statement as the catch point even when a
    later clause matched, so the clause is picked by matching the exception's class names.
    """
    tree = parse_file(path)
    if tree is None:
        return CatchSite("unknown")
    innermost = None
    for node in ast.walk(tree):
        if isinstance(node, (ast.With, ast.AsyncWith)) and node.lineno == line:
            return CatchSite("with")
        if isinstance(node, TRY_NODES) and node.lineno <= line <= node.end_lineno:
            if innermost is None or node.lineno > innermost.lineno:
                innermost = node
    if innermost is None:
        return CatchSite("unknown")
    if innermost.finalbody and line >= innermost.finalbody[0].lineno:
        return CatchSite("finally")
    if not innermost.handlers:
        return CatchSite("unknown")
    clause = next((h for h in innermost.handlers if _matches(h, type_names)), innermost.handlers[0])
    return CatchSite("except", _handler(clause, _scope_of(tree, clause)))


# ---------------------------------------------------------------- helpers

class _Collector(ast.NodeVisitor):
    def __init__(self):
        self.functions: list[Function] = []
        self.handlers: list[Handler] = []
        self.prints: list[tuple[int, str]] = []
        self.variables: dict[str, set[str]] = {}
        self.stack: list[tuple[str, str]] = [("module", "<module>")]   # (kind, qualname)

    @property
    def scope(self) -> str:
        return self.stack[-1][1]

    def _qualname(self, name: str) -> str:
        kind, qual = self.stack[-1]
        if kind == "module":
            return name
        return f"{qual}.{name}" if kind == "class" else f"{qual}.<locals>.{name}"

    def _function(self, node, is_async: bool) -> None:
        for expr in [*node.decorator_list, node.args, node.returns]:
            if expr is not None:
                self.visit(expr)          # evaluated in the enclosing scope
        qual = self._qualname(node.name)
        self.functions.append(Function(
            qualname=qual,
            line=node.lineno,
            first_line=min([node.lineno] + [d.lineno for d in node.decorator_list]),
            is_async=is_async,
            args=_arg_names(node.args),
            complexity=_complexity(node),
            returns=_returns(node),
            docstring=ast.get_docstring(node) is not None,
            decorators=[ast.unparse(d) for d in node.decorator_list],
        ))
        self.stack.append(("function", qual))
        for stmt in node.body:
            self.visit(stmt)
        self.stack.pop()

    def visit_FunctionDef(self, node):
        self._function(node, is_async=False)

    def visit_AsyncFunctionDef(self, node):
        self._function(node, is_async=True)

    def visit_ClassDef(self, node):
        for expr in [*node.decorator_list, *node.bases, *node.keywords]:
            self.visit(expr)
        self.stack.append(("class", self._qualname(node.name)))
        for stmt in node.body:
            self.visit(stmt)
        self.stack.pop()

    def visit_Call(self, node):
        if isinstance(node.func, ast.Name) and node.func.id == "print":
            self.prints.append((node.lineno, self.scope))
        self.generic_visit(node)

    def visit_Name(self, node):
        if isinstance(node.ctx, ast.Store):
            self.variables.setdefault(self.scope, set()).add(node.id)

    def visit_comprehension(self, node):
        # comprehension variables live in the comprehension's own scope
        self.visit(node.iter)
        for cond in node.ifs:
            self.visit(cond)

    def visit_ExceptHandler(self, node):
        if node.name:
            self.variables.setdefault(self.scope, set()).add(node.name)
        self.handlers.append(_handler(node, self.scope))
        self.generic_visit(node)


def _walk_own(body):
    """Walk statements without entering nested functions, classes or lambdas."""
    stack = list(body)
    while stack:
        node = stack.pop()
        yield node
        if not isinstance(node, SCOPES):
            stack.extend(ast.iter_child_nodes(node))


def _complexity(fn) -> int:
    score = 1
    for node in _walk_own(fn.body):
        if isinstance(node, (ast.If, ast.IfExp, ast.For, ast.AsyncFor, ast.While,
                             ast.ExceptHandler, ast.Assert, ast.match_case)):
            score += 1
        elif isinstance(node, ast.comprehension):
            score += 1 + len(node.ifs)
        elif isinstance(node, ast.BoolOp):
            score += len(node.values) - 1
    return score


def _returns(fn) -> str:
    nodes = list(_walk_own(fn.body))
    if any(isinstance(n, (ast.Yield, ast.YieldFrom)) for n in nodes):
        return "yields"
    if any(isinstance(n, ast.Return) and n.value is not None for n in nodes):
        return "value"
    return "none"


def _arg_names(a: ast.arguments) -> list[str]:
    names = [x.arg for x in a.posonlyargs + a.args]
    if a.vararg:
        names.append("*" + a.vararg.arg)
    elif a.kwonlyargs:
        names.append("*")
    names += [x.arg for x in a.kwonlyargs]
    if a.kwarg:
        names.append("**" + a.kwarg.arg)
    return names


def _type_names(expr) -> set[str]:
    return ({n.id for n in ast.walk(expr) if isinstance(n, ast.Name)}
            | {n.attr for n in ast.walk(expr) if isinstance(n, ast.Attribute)})


def _matches(h: ast.ExceptHandler, type_names: frozenset[str]) -> bool:
    return h.type is None or not _type_names(h.type).isdisjoint(type_names)


def _handler(h: ast.ExceptHandler, scope: str) -> Handler:
    return Handler(
        line=h.lineno,
        scope=scope,
        catches=ast.unparse(h.type) if h.type is not None else "",
        broad=h.type is None or not _type_names(h.type).isdisjoint(BROAD),
        verdict=_classify(h),
    )


def _classify(h: ast.ExceptHandler) -> str:
    nodes = list(_walk_own(h.body))
    if any(isinstance(n, ast.Raise) for n in nodes):
        return "reraised"
    if all(_is_trivial(s) for s in h.body):
        return "ignored"
    calls = [n for n in nodes if isinstance(n, ast.Call)]
    names = {_call_name(c) for c in calls}
    with_traceback = any(k.arg == "exc_info" and not (isinstance(k.value, ast.Constant) and not k.value.value)
                         for c in calls for k in c.keywords)
    if with_traceback or names & TRACEBACK_CALLS:
        return "logged"
    if names & LOG_CALLS:
        return "logged-message"
    if "print" in names or any(_writes_to_stream(c) for c in calls):
        return "printed"
    return "fallback"


def _is_trivial(stmt) -> bool:
    if isinstance(stmt, (ast.Pass, ast.Continue, ast.Break)):
        return True
    if isinstance(stmt, ast.Expr) and isinstance(stmt.value, ast.Constant):
        return True     # a docstring-like string or `...`
    if isinstance(stmt, ast.Return):
        return stmt.value is None or (isinstance(stmt.value, ast.Constant) and stmt.value.value is None)
    return False


def _call_name(call: ast.Call) -> str:
    f = call.func
    if isinstance(f, ast.Attribute):
        return f.attr
    return f.id if isinstance(f, ast.Name) else ""


def _writes_to_stream(call: ast.Call) -> bool:
    f = call.func
    return (isinstance(f, ast.Attribute) and f.attr == "write"
            and isinstance(f.value, ast.Attribute) and f.value.attr in ("stdout", "stderr"))


def _scope_of(tree: ast.Module, target: ast.AST) -> str:
    """Qualified name of the function enclosing `target`."""
    collector = _ScopeFinder(target)
    collector.visit(tree)
    return collector.found or "<module>"


class _ScopeFinder(_Collector):
    def __init__(self, target):
        super().__init__()
        self.target = target
        self.found = None

    def visit_ExceptHandler(self, node):
        if node is self.target:
            self.found = self.scope
        self.generic_visit(node)
