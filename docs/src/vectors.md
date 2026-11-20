# Test vectors

Every binding runs the same ground-truth vectors. Behavioral
difference between bindings is a bug, and the shared vectors are the
drill that catches it.

## Layout

```text
tests/vectors/     ground-truth inputs and expected outputs
tests/fixtures/    synthetic mock statement CSVs (per target shape)
```

`tests/vectors/` holds `parse_float.tsv` and `dec.tsv` for the scalar
API, `stream.json` and `stream_filter.json` for the session lifecycle,
and per-topic JSON vectors (`income.json`, `risk_series.json`,
`twr.json`, `wash_sales.json`, …) for every derivation. Fixtures are
named by shape, not by any real account.

Vectors live only in the root `tests/` tree. Bindings reference them by
relative path and never hold copies — a fixture copy inside a binding
is a bug.

## The drill

```sh
cargo test --workspace        # core + FFI tests
scripts/c-smoke.sh            # C consumer linked against the static lib
scripts/bindings-test.sh      # python + go + node + java vs shared vectors
```

`scripts/bindings-test.sh` builds all targets and runs every binding
suite in CI (`ci.yml`) on every push and pull request.

## Adding a rule

A new parsing rule is not accepted without documented grounding in
real statement-export characteristics and synthetic test vectors in the
same change. Vectors are additive; changing an expected value is a
behavioral change and gets reviewed as one.
