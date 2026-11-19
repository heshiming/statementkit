import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import url from "node:url";

import { openSession } from "../lib/index.js";

const VECTORS = path.resolve(
  path.dirname(url.fileURLToPath(import.meta.url)),
  "..", "..", "..", "tests", "vectors",
);

async function runVector(name) {
  const vec = JSON.parse(await readFile(path.join(VECTORS, name), "utf8"));
  const s = await openSession(vec.targets, vec.tickers || []);
  try {
    for (const f of vec.files) {
      s.feed(new Uint8Array(await readFile(path.resolve(VECTORS, f))));
      s.endFile();
    }
    const text = s.report();
    for (const frag of vec.expect) {
      assert.ok(text.includes(frag), `${name}: missing fragment ${frag.slice(0, 60)}`);
    }
  } finally {
    s.close();
  }
}

test("stream vector", () => runVector("stream.json"));
test("stream filter vector", () => runVector("stream_filter.json"));

test("unknown target carries ABI message", async () => {
  await assert.rejects(() => openSession(["nope"]), (e) => {
    assert.match(e.message, /unknown targets: nope/);
    return true;
  });
});
