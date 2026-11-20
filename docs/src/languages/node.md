# Node / TypeScript

```sh
npm install statementkit
```

Raw WASM: the C ABI crate compiled to `wasm32-unknown-unknown`, loaded
by `lib/index.js`. Zero npm dependencies, zero native toolchain.
TypeScript types ship with the package (`lib/index.d.ts`).

## Scalars

```js
import { ready, parseFloat, version } from "statementkit";
await ready();
await parseFloat("12.25");   // 12.25
await parseFloat("1,2");     // rejects with InvalidInputError
```

## Streaming session

```js
import { openSession } from "statementkit";
const s = await openSession(["income", "trades"], ["ABC"]);
s.feed(new Uint8Array(fs.readFileSync("statement.csv")));
s.endFile();
const text = s.report(); // compact JSON; parse with a lossless reader
s.close();
```

The ticker selection argument is optional; without it the report
carries no `filter` fragment. Numeric literals in the report are
unquoted — read them with a decimal-lossless JSON reader
(`json.Number`-style), not bare `JSON.parse`, if precision matters.

Errors are the `StatementKitError` subclasses. WASM traps (possible
only under `panic=abort`, the wasm target's only mode) surface as
`PanicError`.
