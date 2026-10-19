"""End-to-end tests: run fixtures through `python -m whathappened run` with every built engine."""
import json
import os
import subprocess
import sys
import tempfile
import unittest

if sys.version_info < (3, 12):
    raise unittest.SkipTest("whathappened run needs Python 3.12+")

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
FIXTURES = os.path.join(HERE, "fixtures")
sys.path.insert(0, ROOT)

from whathappened.tracer import engines  # noqa: E402

# (function that caught it, exception type) -> (count, kind, hidden)
EXPECTED = {
    ("broad_print", "ValueError"): (1, "except", True),
    ("bare_pass", "ValueError"): (1, "except", True),
    ("logged_message_only", "ValueError"): (1, "except", True),
    ("reraised_then_caught", "ValueError"): (1, "except", True),
    ("second_clause", "TypeError"): (1, "except", True),
    ("finally_return", "ValueError"): (1, "finally", True),
    ("caller_of_converted", "RuntimeError"): (1, "except", True),
    ("from_library", "JSONDecodeError"): (1, "except", True),
    ("from_c", "ValueError"): (1, "except", True),
    ("in_loop", "ValueError"): (50, "except", True),
    ("Thread._bootstrap_inner", "ValueError"): (1, "thread", True),
    ("generator", "ValueError"): (1, "except", True),
    ("logged_with_traceback", "ValueError"): (1, "except", False),
    ("suppressed", "TypeError"): (1, "with", False),
    ("specific_fallback", "KeyError"): (1, "except", False),
}


def run(fixture, *options, args=()):
    """Run a fixture under whathappened; return (exit code, stdout, stderr, JSON report)."""
    with tempfile.TemporaryDirectory() as tmp:
        report_path = os.path.join(tmp, "report.json")
        proc = subprocess.run(
            [sys.executable, "-m", "whathappened", "run", "--json", report_path, *options,
             os.path.join(FIXTURES, fixture), *args],
            cwd=ROOT, capture_output=True, text=True, encoding="utf-8",
            env={**os.environ, "PYTHONIOENCODING": "utf-8", "PYTHONDONTWRITEBYTECODE": "1"})
        with open(report_path, encoding="utf-8") as f:
            report = json.load(f)["run"]
    return proc.returncode, proc.stdout, proc.stderr, report


def swallowed(report):
    return {(s["caught"]["function"], s["type"]): (s["count"], s["kind"], s["hidden"])
            for s in report["swallowed"]}


def fixture_line(fixture, text):
    with open(os.path.join(FIXTURES, fixture), encoding="utf-8") as f:
        return next(n for n, line in enumerate(f, 1) if line.strip() == text)


class RunTests(unittest.TestCase):
    def test_every_case_with_every_engine(self):
        for engine in engines():
            with self.subTest(engine=engine):
                code, out, err, report = run("cases.py", "--engine", engine)
                self.assertEqual(code, 0, err)
                self.assertEqual(report["engine"], engine)
                self.assertEqual(out.splitlines()[-1], "done")
                self.assertEqual(swallowed(report), EXPECTED)
                self.assertEqual(report["summary"], {"hidden_errors": 61, "caught_on_purpose": 3})
                self.assertEqual([n["function"] for n in report["never_ran"]], ["never_called"])

    def test_exceptions_only(self):
        for engine in engines():
            with self.subTest(engine=engine):
                _, _, _, report = run("cases.py", "--exceptions-only", "--engine", engine)
                self.assertEqual(swallowed(report), EXPECTED)
                self.assertEqual(report["functions"], [])

    def test_engines_agree_on_everything(self):
        if len(engines()) < 2:
            self.skipTest("the Rust engine is not built")
        reports = [run("cases.py", "--engine", e)[3] for e in engines()]
        swallows = [sorted(json.dumps(s, sort_keys=True) for s in r["swallowed"]) for r in reports]
        calls = [sorted((f["function"], f["calls"]) for f in r["functions"]) for r in reports]
        self.assertEqual(swallows[0], swallows[1])
        self.assertEqual(calls[0], calls[1])

    def test_points_at_the_clause_that_matched(self):
        _, _, _, report = run("cases.py")
        caught = next(s["caught"] for s in report["swallowed"] if s["caught"]["function"] == "second_clause")
        self.assertEqual(caught["line"], fixture_line("cases.py", "except TypeError:"))

    def test_script_output_is_unchanged(self):
        direct = subprocess.run([sys.executable, os.path.join(FIXTURES, "cases.py")],
                                capture_output=True, text=True, encoding="utf-8")
        self.assertEqual(run("cases.py")[1], direct.stdout)

    def test_crash_keeps_pythons_traceback(self):
        code, _, err, report = run("crash.py")
        self.assertEqual(code, 1)
        self.assertEqual(report["status"], "crash")
        self.assertEqual(report["error"], "RuntimeError: boom")
        self.assertEqual(report["error_at"]["function"], "explode")
        self.assertIn('crash.py", line 2, in explode', err)
        self.assertNotIn("runpy", err)
        self.assertNotIn("tracer.py", err)

    def test_modern_script_runs_like_plain_python(self):
        code, out, _, report = run("modern.py", args=("a", "b"))
        self.assertEqual(code, 4)
        self.assertEqual(report["status"], "exit")
        self.assertEqual(json.loads(out), {"args": ["a", "b"], "pickled": True, "frame_freed": True})

    def test_fail_on_swallowed(self):
        self.assertEqual(run("cases.py", "--fail-on-swallowed")[0], 3)


if __name__ == "__main__":
    unittest.main()
