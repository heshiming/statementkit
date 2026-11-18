# Node / TypeScript binding (reserved)

Plan:

1. `bindings/node` npm package, TypeScript-first (`.d.ts` generated, not
   handwritten).
2. Backend decision when API is real: `wasm-bindgen` (pure-WASM install,
   browser-safe) vs `napi-rs` (native, faster). Both sit on the core
   crate; the choice does not change the JS API.

Rules: thin shell only — no parsing logic here; core test vectors re-run
through the binding.
