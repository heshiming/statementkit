"""Shared-vector test: bindings/python must agree with tests/vectors."""

import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import statementkit  # noqa: E402

VECTORS = pathlib.Path(__file__).resolve().parents[2] / "tests" / "vectors" / "parse_float.tsv"

_EXPECTS = {
    "ok": None,
    "empty": statementkit.EmptyInputError,
    "invalid": statementkit.InvalidInputError,
    "null": statementkit.NullInputError,
}


class ParseFloatVectors(unittest.TestCase):
    def test_vectors(self):
        rows = 0
        for line in VECTORS.read_text().splitlines():
            if not line or line.startswith("#"):
                continue
            case, text, *rest = line.split("\t")
            expect = rest[0] if rest else ""
            rows += 1
            with self.subTest(case=case, input=text):
                if case == "ok":
                    self.assertEqual(statementkit.parse_float(text), float(expect))
                else:
                    with self.assertRaises(_EXPECTS[case]):
                        statementkit.parse_float(None if case == "null" else text)
        self.assertGreaterEqual(rows, 20)

    def test_version(self):
        self.assertRegex(statementkit.version(), r"^\d+\.\d+\.\d+")


if __name__ == "__main__":
    unittest.main()
