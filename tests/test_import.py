"""End-to-end tests: programs that `import hushed` and run with plain `python`."""
import json
import os
import sys
import tempfile
import textwrap
import unittest

from support import FIXTURES, hushed, python
from test_run import EXPECTED, swallowed

if sys.version_info < (3, 12):
    raise unittest.SkipTest("hushed needs Python 3.12+")


def run(source, env=None):
    """Write `source` to a script, run it with python; return (exit code, stdout, stderr, report)."""
    with tempfile.TemporaryDirectory() as tmp:
        script = os.path.join(tmp, "app.py")
        with open(script, "w", encoding="utf-8") as f:
            f.write(textwrap.dedent(source))
        return run_file(script, env)


def run_file(script, env=None):
    with tempfile.TemporaryDirectory() as tmp:
        report_path = os.path.join(tmp, "report.json")
        proc = python(script, env={"HUSHED_JSON": report_path, **(env or {})})
        report = None
        if os.path.exists(report_path):
            with open(report_path, encoding="utf-8") as f:
                report = json.load(f)["run"]
    return proc.returncode, proc.stdout, proc.stderr, report


class ImportTests(unittest.TestCase):
    def test_reports_when_the_program_exits(self):
        code, out, err, report = run_file(os.path.join(FIXTURES, "imported.py"))
        self.assertEqual(code, 0, err)
        self.assertEqual(out.splitlines()[-1], "done")
        self.assertEqual(swallowed(report), EXPECTED)
        self.assertEqual(report["status"], "ok")
        self.assertEqual([n["function"] for n in report["never_ran"]], ["never_called"])
        self.assertIn("hushed: ", err)

    def test_crash(self):
        code, _, err, report = run("""\
            import hushed

            def explode():
                raise RuntimeError("boom")

            explode()
            """)
        self.assertEqual(code, 1)
        self.assertIn('line 4, in explode', err)  # Python's own traceback is still printed
        self.assertEqual(report["status"], "crash")
        self.assertEqual(report["error"], "RuntimeError: boom")
        self.assertEqual(report["error_at"]["function"], "explode")

    def test_exit_code(self):
        code, _, _, report = run("""\
            import hushed, sys

            def main():
                try:
                    sys.exit(9)          # caught, so it doesn't end the program
                except SystemExit:
                    pass
                return 4

            sys.exit(main())
            """)
        self.assertEqual(code, 4)
        self.assertEqual((report["status"], report["exit_code"]), ("exit", 4))

    def test_caught_exit_is_not_the_end(self):
        _, _, _, report = run("""\
            import hushed, sys
            try:
                sys.exit(3)
            except SystemExit:
                pass
            """)
        self.assertEqual(report["status"], "ok")

    def test_switched_off(self):
        code, _, err, report = run("import hushed\n", env={"HUSHED": "0"})
        self.assertEqual((code, err, report), (0, "", None))

    def test_hushed_run_reports_once(self):
        proc = hushed("run", os.path.join(FIXTURES, "imported.py"))
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stderr.count("hushed: "), 1, proc.stderr)

    def test_multiprocessing_children_stay_quiet(self):
        code, _, err, report = run("""\
            import hushed
            import multiprocessing

            def work():
                try:
                    int("x")
                except ValueError:
                    pass

            if __name__ == "__main__":
                child = multiprocessing.get_context("spawn").Process(target=work)
                child.start()
                child.join()
                work()
            """)
        self.assertEqual(code, 0, err)
        self.assertEqual(err.count("hushed: "), 1, err)
        self.assertEqual(report["summary"]["hidden_errors"], 1)

    @unittest.skipUnless(hasattr(os, "fork"), "needs os.fork")
    def test_forked_children_stay_quiet(self):
        code, _, err, _ = run("""\
            import hushed, os, sys
            pid = os.fork()
            if pid == 0:
                sys.exit(0)       # a normal exit, so atexit handlers run in the child too
            os.waitpid(pid, 0)
            """)
        self.assertEqual(code, 0, err)
        self.assertEqual(err.count("hushed: "), 1, err)


if __name__ == "__main__":
    unittest.main()
