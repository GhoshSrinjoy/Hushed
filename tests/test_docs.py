"""The code and output shown in the docs are what the examples really contain and print."""
import os
import re
import sys
import unittest

from support import ROOT

if sys.version_info < (3, 12):
    raise unittest.SkipTest("hushed needs Python 3.12+")

sys.path.insert(0, os.path.join(ROOT, "scripts"))
import update_docs  # noqa: E402

TIMING_ROW = re.compile(r"^ +\d+\.\d+s  .*\n", re.M)  # their order shifts with the times
CARETS = re.compile(r"^ *[~^]+ *\n", re.M)  # traceback markers differ between Python versions
TIMES = re.compile(r"\d+\.\d+s")


def normalize(text):
    return TIMES.sub("0.00s", CARETS.sub("", TIMING_ROW.sub("", text)))


class DocsTests(unittest.TestCase):
    def test_docs_show_the_real_code_and_output(self):
        with update_docs.workspace() as work:
            for doc in update_docs.DOCS:
                with open(os.path.join(ROOT, doc), encoding="utf-8") as f:
                    text = f.read()
                with self.subTest(doc=doc):
                    self.assertEqual(normalize(text), normalize(update_docs.fill(text, work)),
                                     "the docs are out of date: run python scripts/update_docs.py")


if __name__ == "__main__":
    unittest.main()
