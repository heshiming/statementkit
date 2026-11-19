"""Session vector test: shared vectors from tests/vectors/."""
import json
import pathlib
import sys
from decimal import Decimal

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from statementkit import Session  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parents[2]
VECTORS = ROOT / "tests" / "vectors"

for name in ("stream.json", "stream_filter.json"):
    vec = json.loads((VECTORS / name).read_text())
    files = [(VECTORS / f).resolve() for f in vec["files"]]
    with Session(vec["targets"], vec.get("tickers") or []) as s:
        for f in files:
            s.feed(f.read_bytes())
            s.end_file()
        text = s.report()
        for frag in vec["expect"]:
            assert frag in text, f"{name}: missing fragment {frag[:60]!r}"
        parsed = s.report_json()
        assert parsed["files"] == len(files)

# numeric literals must survive as exact decimals
vec = json.loads((VECTORS / "stream_filter.json").read_text())
with Session(vec["targets"], vec["tickers"]) as s:
    for f in vec["files"]:
        s.feed((VECTORS / f).resolve().read_bytes())
        s.end_file()
    p = s.report_json()["filter"]["partition"]
    assert p["grand_total"] == 9918, p
    assert isinstance(p["grand_total"], (int, Decimal))
    assert p["selected_roots"] == -96 and p["every_other_root"] == -1

# unknown target carries the ABI message
try:
    Session(["nope"])
    raise AssertionError("unknown target accepted")
except Exception as e:
    assert "unknown targets: nope" in str(e), e

print("python session vectors ok")
