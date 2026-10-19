"""End-to-end tests: run fixtures through `python -m hushed run`."""
import json
import os
import subprocess
import sys
import tempfile
import unittest

from support import FIXTURES, hushed

if sys.version_info < (3, 12):
    raise unittest.SkipTest("hushed needs Python 3.12+")

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
    """Run a fixture under hushed; return (exit code, stdout, stderr, JSON report)."""
    with tempfile.TemporaryDirectory() as tmp:
        report_path = os.path.join(tmp, "report.json")
        proc = hushed("run", "--json", report_path, *options, os.path.join(FIXTURES, fixture), *args)
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
    def test_every_case(self):
        code, out, err, report = run("cases.py")
        self.assertEqual(code, 0, err)
        self.assertEqual(out.splitlines()[-1], "done")
        self.assertEqual(swallowed(report), EXPECTED)
        self.assertEqual(report["summary"], {"hidden_errors": 61, "caught_on_purpose": 3})
        self.assertEqual([n["function"] for n in report["never_ran"]], ["never_called"])
        self.assertIn("in_loop", {f["function"] for f in report["functions"]})

    def test_exceptions_only(self):
        _, _, _, report = run("cases.py", "--exceptions-only")
        self.assertEqual(swallowed(report), EXPECTED)
        self.assertEqual(report["functions"], [])

    def test_points_at_the_clause_that_matched(self):
        _, _, _, report = run("cases.py")
        caught = next(s["caught"] for s in report["swallowed"] if s["caught"]["function"] == "second_clause")
        self.assertEqual(caught["line"], fixture_line("cases.py", "except TypeError:"))

    def test_report_is_stable(self):
        self.assertEqual(run("cases.py")[3]["swallowed"], run("cases.py")[3]["swallowed"])

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

    def test_modern_script_runs_like_plain_python(self):
        code, out, _, report = run("modern.py", args=("a", "--flag"))
        self.assertEqual(code, 4)
        self.assertEqual(report["status"], "exit")
        self.assertEqual(json.loads(out), {"args": ["a", "--flag"], "pickled": True, "frame_freed": True})

    def test_fail_on_swallowed(self):
        self.assertEqual(run("cases.py", "--fail-on-swallowed")[0], 3)

    def test_command_line_errors(self):
        self.assertEqual(hushed("run", "no_such_script.py").returncode, 2)
        self.assertEqual(hushed("run").returncode, 2)
        self.assertEqual(hushed("--help").returncode, 0)


if __name__ == "__main__":
    unittest.main()
