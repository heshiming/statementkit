# Node / TypeScript binding — implemented (raw WASM)

`lib/index.js` loads `statementkit_c.wasm` — the C ABI crate compiled with
`cargo build --release -p statementkit-c --target wasm32-unknown-unknown`
(see `package.json` `build:wasm`; a `wasm/` copy ships in the npm package)
— and marshals strings through linear memory via the `sk_wasm_alloc` /
`sk_wasm_free` exports. Zero npm dependencies, zero native toolchain.

```js
import { ready, parseFloat, version } from "statementkit";
await ready();
await parseFloat("12.25");   // 12.25
await parseFloat("1,2");     // rejects with InvalidInputError
```

`lib/index.d.ts` carries the TypeScript types; hand-written while the API
is two functions, codegen (wasm-bindgen) arrives with a bigger surface.
Errors are the `StatementKitError` subclasses; WASM traps (possible only
under `panic=abort`, the wasm target's only mode) surface as `PanicError`.

Test: `node --test test/parse_float.test.js` (shared vectors).
