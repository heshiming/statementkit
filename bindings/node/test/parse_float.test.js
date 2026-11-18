// Shared-vector test: bindings/node must agree with tests/vectors.
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  ready,
  parseFloat as parse,
  version,
  NullInputError,
  EmptyInputError,
  InvalidInputError,
} from "../lib/index.js";

const VECTORS = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..", "..", "..", "tests", "vectors", "parse_float.tsv",
);

const THROWS = { empty: EmptyInputError, invalid: InvalidInputError, null: NullInputError };

test("parse_float shared vectors", async () => {
  await ready();
  const lines = (await readFile(VECTORS, "utf8")).split("\n");
  let rows = 0;
  for (const line of lines) {
    if (!line || line.startsWith("#")) continue;
    const [kind, input, expect] = line.split("\t");
    rows += 1;
    await t_row(kind, input, expect);
  }
  assert.ok(rows >= 20, `only ${rows} vector rows`);
});

async function t_row(kind, input, expect) {
  if (kind === "ok") {
    const got = await parse(input);
    assert.equal(got, Number(expect), `input: ${JSON.stringify(input)}`);
  } else {
    await assert.rejects(
      () => parse(kind === "null" ? null : input),
      THROWS[kind],
      `input: ${JSON.stringify(input)}`,
    );
  }
}

test("version looks semver", async () => {
  const v = await version();
  assert.match(v, /^\d+\.\d+\.\d+$/);
});
