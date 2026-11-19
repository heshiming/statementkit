// statementkit — raw-WASM binding. Loads statementkit_c.wasm (the C ABI
// crate compiled for wasm32-unknown-unknown) and marshals strings through
// linear memory using the sk_wasm_alloc/sk_wasm_free exports. No
// dependencies, no build step beyond `cargo build --target wasm32...`.

import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));

export class StatementKitError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}
export class NullInputError extends StatementKitError {}
export class EmptyInputError extends StatementKitError {}
export class InvalidInputError extends StatementKitError {}
export class PanicError extends StatementKitError {}

const ERRORS = {
  1: NullInputError,
  2: EmptyInputError,
  3: InvalidInputError,
  4: PanicError,
};

function wasmCandidates() {
  const candidates = [process.env.STATEMENTKIT_WASM];
  candidates.push(path.resolve(HERE, "..", "wasm", "statementkit_c.wasm"));
  candidates.push(
    path.resolve(HERE, "..", "..", "..", "target", "wasm32-unknown-unknown", "release", "statementkit_c.wasm"),
  );
  return candidates.filter(Boolean);
}

let instance = null;
let loading = null;

async function instantiate() {
  const file = wasmCandidates().find((p) => existsSync(p));
  if (!file) {
    throw new Error(
      "statementkit wasm not found; run `cargo build --release -p statementkit-c " +
        "--target wasm32-unknown-unknown` or set $STATEMENTKIT_WASM",
    );
  }
  const bytes = await readFile(file);
  const result = await WebAssembly.instantiate(bytes, {});
  return result.instance;
}

/** Await once before the synchronous-looking helpers; instantiation is async. */
export function ready() {
  if (!loading) loading = instantiate().then((i) => (instance = i));
  return loading;
}

const memory = () => instance.exports.memory.buffer;

function writeCString(text) {
  const bytes = new TextEncoder().encode(text);
  const ptr = instance.exports.sk_wasm_alloc(bytes.length + 1);
  if (ptr === 0) throw new RangeError("statementkit: wasm allocation failed");
  new Uint8Array(memory()).set(bytes, ptr);
  new Uint8Array(memory())[ptr + bytes.length] = 0;
  return { ptr, len: bytes.length + 1 };
}

function readCString(ptr) {
  const view = new Uint8Array(memory());
  let end = ptr;
  while (view[end] !== 0) end += 1;
  return new TextDecoder().decode(view.subarray(ptr, end));
}

function check(status) {
  if (status !== 0) {
    const ErrorClass = ERRORS[status] ?? PanicError;
    throw new ErrorClass(status, `sk_parse_float failed with status ${status}`);
  }
}

/** Parse a bare float literal. Resolves with the value; rejects with a
 *  StatementKitError subclass (never a bare WASM trap). */
export async function parseFloat(input) {
  if (input === null || input === undefined) {
    throw new NullInputError(1, "input is null");
  }
  await ready();
  const text = String(input);
  const inputStr = writeCString(text);
  const outPtr = instance.exports.sk_wasm_alloc(8);
  try {
    let status;
    try {
      status = instance.exports.sk_parse_float(inputStr.ptr, outPtr);
    } catch (trap) {
      throw new PanicError(4, `wasm trap at the FFI boundary: ${trap.message}`);
    }
    check(status);
    return new DataView(memory(), outPtr, 8).getFloat64(0, true);
  } finally {
    instance.exports.sk_wasm_free(inputStr.ptr, inputStr.len);
    instance.exports.sk_wasm_free(outPtr, 8);
  }
}

/** Library version, e.g. "0.1.0". */
export async function version() {
  await ready();
  return readCString(instance.exports.sk_version());
}

function allocWasm(len) {
  const ptr = instance.exports.sk_wasm_alloc(len);
  if (ptr === 0) throw new RangeError("statementkit: wasm allocation failed");
  return ptr;
}

function writeStrArray(items) {
  const strs = items.map((s) => writeCString(s));
  if (!strs.length) return { ptr: 0, strs, arrLen: 0 };
  const arr = allocWasm(4 * strs.length);
  const view = new DataView(memory());
  strs.forEach((s, i) => view.setUint32(arr + 4 * i, s.ptr, true));
  return { ptr: arr, strs, arrLen: 4 * strs.length };
}

function freeStrArray(a) {
  for (const s of a.strs) instance.exports.sk_wasm_free(s.ptr, s.len);
  if (a.arrLen) instance.exports.sk_wasm_free(a.ptr, a.arrLen);
}

/** Push streaming session. Await `ready()` (or use `openSession`)
 *  before use. report() returns the compact JSON text; parse with a
 *  lossless reader to keep numeric literals exact — JSON.parse coerces
 *  to float64. */
export class Session {
  constructor(targets = [], tickers = []) {
    const t = writeStrArray(targets);
    const x = writeStrArray(tickers);
    const err = allocWasm(512);
    const out = allocWasm(4);
    let status;
    try {
      status = instance.exports.sk_session_open(out, t.ptr, targets.length,
        x.ptr, tickers.length, err, 512);
    } catch (trap) {
      throw new PanicError(4, `wasm trap at the FFI boundary: ${trap.message}`);
    } finally {
      freeStrArray(t);
      freeStrArray(x);
    }
    const handle = new DataView(memory()).getUint32(out, true);
    instance.exports.sk_wasm_free(out, 4);
    const message = readCString(err);
    instance.exports.sk_wasm_free(err, 512);
    if (status !== 0) {
      const ErrorClass = ERRORS[status] ?? PanicError;
      throw new ErrorClass(status, message || `session open failed (${status})`);
    }
    this._p = handle;
  }

  _check(status) {
    if (status !== 0) {
      const ErrorClass = ERRORS[status] ?? PanicError;
      throw new ErrorClass(status, `session call failed (${status})`);
    }
  }

  /** Feed a Uint8Array chunk of the current file. */
  feed(bytes) {
    if (!bytes.length) return;
    const ptr = allocWasm(bytes.length);
    try {
      new Uint8Array(memory()).set(bytes, ptr);
      this._check(instance.exports.sk_session_feed(this._p, ptr, bytes.length));
    } finally {
      instance.exports.sk_wasm_free(ptr, bytes.length);
    }
  }

  endFile() {
    this._check(instance.exports.sk_session_end_file(this._p));
  }

  report() {
    const out = allocWasm(4);
    let len;
    try {
      this._check(instance.exports.sk_session_report_len(this._p, out));
      len = new DataView(memory()).getUint32(out, true);
    } finally {
      instance.exports.sk_wasm_free(out, 4);
    }
    const buf = allocWasm(len + 1);
    try {
      this._check(instance.exports.sk_session_report(this._p, buf, len + 1));
      return readCString(buf);
    } finally {
      instance.exports.sk_wasm_free(buf, len + 1);
    }
  }

  close() {
    if (this._p) {
      instance.exports.sk_session_close(this._p);
      this._p = 0;
    }
  }
}

/** Instantiate after the wasm module is loaded. */
export async function openSession(targets, tickers = []) {
  await ready();
  return new Session(targets, tickers);
}
