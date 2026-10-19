"""Tests for `hushed analyze`: functions, complexity and except-block verdicts."""
import json
import os
import sys
import tempfile
import textwrap
import unittest

from support import FIXTURES, hushed

if sys.version_info < (3, 12):
    raise unittest.SkipTest("hushed needs Python 3.12+")

VERDICTS = {
    "pass": "ignored",
    "continue": "ignored",
    "return None": "ignored",
    "print(e)": "printed",
    "sys.stderr.write(str(e))": "printed",
    "log.error('failed: %s', e)": "logged-message",
    "warnings.warn(str(e))": "logged-message",
    "log.error('failed', exc_info=True)": "logged",
    "log.error('failed', exc_info=False)": "logged-message",
    "log.exception('failed')": "logged",
    "traceback.print_exc()": "logged",
    "raise": "reraised",
    "raise RuntimeError('x') from e": "reraised",
    "value = default": "fallback",
}


def analyze(*paths):
    with tempfile.TemporaryDirectory() as tmp:
        report = os.path.join(tmp, "report.json")
        proc = hushed("analyze", "--json", report, *paths)
        with open(report, encoding="utf-8") as f:
            return proc.returncode, json.load(f)["files"]


def analyze_sources(sources):
    """Write each snippet to its own file, analyze them in one run, return {name: file report}."""
    with tempfile.TemporaryDirectory() as tmp:
        for name, source in sources.items():
            with open(os.path.join(tmp, f"{name}.py"), "w", encoding="utf-8") as f:
                f.write(textwrap.dedent(source))
        _, files = analyze(tmp)
    return {os.path.splitext(os.path.basename(f["path"]))[0]: f for f in files}


def handler_snippet(body, catches="Exception"):
    clause = f"except {catches}:" if catches else "except:"
    return "try:\n    work()\n" + clause + "\n    " + body + "\n"


class SampleFileTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        code, files = analyze(os.path.join(FIXTURES, "analyze_sample.py"))
        assert code == 0
        cls.report = files[0]

    def test_functions_and_complexity(self):
        self.assertEqual({f["qualname"]: f["complexity"] for f in self.report["functions"]}, {
            "calculate_fibonacci": 2,
            "process_data": 5,
            "process_data.<locals>.calculate_average": 2,
            "complex_operation": 5,
            "main": 6,
        })

    def test_prints_after_a_nested_function_stay_in_their_scope(self):
        scopes = {line: scope for line, scope in self.report["prints"]}
        self.assertEqual(scopes[47], "process_data")
        self.assertEqual(scopes[51], "process_data")
        self.assertEqual(self.report["variables"]["process_data"], ["e", "filtered_data", "result"])

    def test_except_blocks(self):
        handlers = self.report["handlers"]
        hiding = [(h["line"], h["scope"]) for h in handlers if h["hides_errors"]]
        self.assertEqual(hiding, [(91, "main"), (99, "main"), (108, "main"), (114, "main")])
        reraising = [(h["line"], h["scope"]) for h in handlers if h["verdict"] == "reraised"]
        self.assertEqual(reraising, [(50, "process_data")])


class VerdictTests(unittest.TestCase):
    def test_verdicts(self):
        reports = analyze_sources({f"case{i}": handler_snippet(body) for i, body in enumerate(VERDICTS)})
        for i, (body, expected) in enumerate(VERDICTS.items()):
            with self.subTest(body=body):
                (handler,) = reports[f"case{i}"]["handlers"]
                self.assertEqual(handler["verdict"], expected)

    def test_broad(self):
        catches = {"bare": "", "exception": "Exception", "tuple": "(ValueError, BaseException)",
                   "specific": "ValueError", "star": "* OSError"}
        reports = analyze_sources({name: handler_snippet("pass", c) for name, c in catches.items()})
        broad = {name: r["handlers"][0]["broad"] for name, r in reports.items()}
        self.assertEqual(broad, {"bare": True, "exception": True, "tuple": True, "specific": False, "star": False})
        self.assertEqual(reports["star"]["handlers"][0]["clause"], "except* OSError")

    def test_broad_fallback_hides_errors_but_specific_does_not(self):
        reports = analyze_sources({"broad": handler_snippet("y = 0", "Exception"),
                                   "specific": handler_snippet("y = 0", "KeyError")})
        self.assertTrue(reports["broad"]["handlers"][0]["hides_errors"])
        self.assertFalse(reports["specific"]["handlers"][0]["hides_errors"])


class StructureTests(unittest.TestCase):
    def test_qualified_names(self):
        (report,) = analyze_sources({"shop": """
            class Shop:
                @property
                def total(self):
                    def tax():
                        return 1
                    return tax()

            async def fetch(url, /, *, timeout=1, **kw):
                yield url
        """}).values()
        functions = {f["qualname"]: f for f in report["functions"]}
        self.assertEqual(set(functions), {"Shop.total", "Shop.total.<locals>.tax", "fetch"})
        self.assertEqual(functions["Shop.total"]["decorators"], ["property"])
        self.assertEqual(functions["Shop.total"]["first_line"], functions["Shop.total"]["line"] - 1)
        self.assertTrue(functions["fetch"]["is_async"])
        self.assertEqual(functions["fetch"]["args"], ["url", "*", "timeout", "**kw"])
        self.assertEqual(functions["fetch"]["returns"], "yields")

    def test_syntax_error_is_reported_not_raised(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = os.path.join(tmp, "broken.py")
            with open(path, "w", encoding="utf-8") as f:
                f.write("def broken(:\n")
            code, files = analyze(path)
        self.assertEqual(code, 1)
        self.assertIn("syntax error at line 1", files[0]["error"])


if __name__ == "__main__":
    unittest.main()
