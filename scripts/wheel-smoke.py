#!/usr/bin/env python3
"""Wheel smoke: the pip-installed wheel must pass the shared vectors with
its bundled native library. Usage: wheel-smoke.py <vectors.tsv>"""

import os
import pathlib
import sys
import unittest

assert not os.environ.get("STATEMENTKIT_LIB"), "env override must be unset"

import statementkit

assert statementkit.__file__, "installed package must have __file__"
PKG_DIR = pathlib.Path(statementkit.__file__).resolve().parent


class WheelSmoke(unittest.TestCase):
    def test_bundled_library(self):
        hits = [p for pat in ("*statementkit_c*.so", "*statementkit_c*.dylib",
                              "*statementkit_c*.dll", "*statementkit_c*.pyd")
                for p in PKG_DIR.glob(pat)]
        self.assertTrue(hits, f"no bundled native lib in {PKG_DIR}")

    def test_vectors(self):
        vectors = pathlib.Path(sys.argv[1])
        expects = {"ok": None, "empty": statementkit.EmptyInputError,
                   "invalid": statementkit.InvalidInputError,
                   "null": statementkit.NullInputError}
        rows = 0
        for line in vectors.read_text().splitlines():
            if not line or line.startswith("#"):
                continue
            case, text, *rest = line.split("\t")
            expect = rest[0] if rest else ""
            rows += 1
            with self.subTest(case=case, input=text):
                if case == "ok":
                    self.assertEqual(statementkit.parse_float(text), float(expect))
                else:
                    with self.assertRaises(expects[case]):
                        statementkit.parse_float(None if case == "null" else text)
        self.assertGreaterEqual(rows, 20)

    def test_version(self):
        self.assertRegex(statementkit.version(), r"^\d+\.\d+\.\d+")


if __name__ == "__main__":
    unittest.main(argv=[sys.argv[0]], verbosity=2)
