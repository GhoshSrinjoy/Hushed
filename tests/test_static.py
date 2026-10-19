"""Tests for the static side: functions, complexity and except-block verdicts."""
import os
import sys
import tempfile
import textwrap
import unittest

if sys.version_info < (3, 12):
    raise unittest.SkipTest("whathappened needs Python 3.12+")

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, ROOT)

from whathappened import static  # noqa: E402


def analyze(source):
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, "snippet.py")
        with open(path, "w", encoding="utf-8") as f:
            f.write(textwrap.dedent(source))
        return static.analyze_file(path)


def verdict(body, catches="Exception"):
    clause = f"except {catches}:" if catches else "except:"
    source = "try:\n    work()\n" + clause + "\n" + textwrap.indent(textwrap.dedent(body), "    ")
    (handler,) = analyze(source).handlers
    return handler.verdict, handler.broad


class ExampleScriptTests(unittest.TestCase):
    def setUp(self):
        self.report = static.analyze_file(os.path.join(ROOT, "examples", "test_script.py"))

    def test_functions_and_complexity(self):
        self.assertEqual({f.qualname: f.complexity for f in self.report.functions}, {
            "calculate_fibonacci": 2,
            "process_data": 5,
            "process_data.<locals>.calculate_average": 2,
            "complex_operation": 5,
            "main": 6,
        })

    def test_prints_after_a_nested_function_stay_in_their_scope(self):
        scopes = dict(self.report.prints)
        self.assertEqual(scopes[47], "process_data")
        self.assertEqual(scopes[51], "process_data")
        self.assertNotIn("x", self.report.variables["process_data"])   # comprehension variable

    def test_except_blocks(self):
        hiding = [(h.line, h.scope) for h in self.report.handlers if h.hides_errors]
        self.assertEqual(hiding, [(91, "main"), (99, "main"), (108, "main"), (114, "main")])
        reraising = [h for h in self.report.handlers if h.verdict == "reraised"]
        self.assertEqual([(h.line, h.scope) for h in reraising], [(50, "process_data")])


class VerdictTests(unittest.TestCase):
    def test_verdicts(self):
        cases = {
            "pass": "ignored",
            "continue": "ignored",
            "return None": "ignored",
            "print(e)": "printed",
            "sys.stderr.write(str(e))": "printed",
            "log.error('failed: %s', e)": "logged-message",
            "warnings.warn(str(e))": "logged-message",
            "log.error('failed', exc_info=True)": "logged",
            "log.exception('failed')": "logged",
            "traceback.print_exc()": "logged",
            "raise": "reraised",
            "raise RuntimeError('x') from e": "reraised",
            "value = default": "fallback",
        }
        for body, expected in cases.items():
            with self.subTest(body=body):
                self.assertEqual(verdict(body)[0], expected)

    def test_broad(self):
        self.assertTrue(verdict("pass", catches="")[1])
        self.assertTrue(verdict("pass", catches="Exception")[1])
        self.assertTrue(verdict("pass", catches="(ValueError, BaseException)")[1])
        self.assertFalse(verdict("pass", catches="ValueError")[1])

    def test_broad_fallback_hides_errors_but_specific_does_not(self):
        broad = analyze("try:\n    x()\nexcept Exception:\n    y = 0\n").handlers[0]
        specific = analyze("try:\n    x()\nexcept KeyError:\n    y = 0\n").handlers[0]
        self.assertTrue(broad.hides_errors)
        self.assertFalse(specific.hides_errors)


class StructureTests(unittest.TestCase):
    def test_qualified_names(self):
        report = analyze("""
            class Shop:
                @property
                def total(self):
                    def tax():
                        return 1
                    return tax()

            async def fetch(url, *, timeout=1, **kw):
                yield url
        """)
        functions = {f.qualname: f for f in report.functions}
        self.assertEqual(set(functions), {"Shop.total", "Shop.total.<locals>.tax", "fetch"})
        self.assertEqual(functions["Shop.total"].decorators, ["property"])
        self.assertEqual(functions["Shop.total"].first_line, functions["Shop.total"].line - 1)
        self.assertTrue(functions["fetch"].is_async)
        self.assertEqual(functions["fetch"].args, ["url", "*", "timeout", "**kw"])
        self.assertEqual(functions["fetch"].returns, "yields")

    def test_catch_site_picks_the_matching_clause(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = os.path.join(tmp, "s.py")
            with open(path, "w", encoding="utf-8") as f:
                f.write("try:\n    x()\nexcept ValueError:\n    pass\nexcept TypeError:\n    print(1)\n")
            site = static.catch_site(path, 3, frozenset({"TypeError", "Exception", "BaseException"}))
        self.assertEqual((site.kind, site.handler.line, site.handler.verdict), ("except", 5, "printed"))

    def test_syntax_error_is_reported_not_raised(self):
        self.assertIn("syntax error at line 1", analyze("def broken(:\n").error)


if __name__ == "__main__":
    unittest.main()
