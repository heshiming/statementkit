# API references

This site covers concepts and guides. Signature-level reference docs
live where each ecosystem expects them:

| Language | Install | Reference |
|----------|---------|-----------|
| Rust | `cargo add statementkit` | [docs.rs/statementkit](https://docs.rs/statementkit) |
| Python | `pip install statementkit` | [PyPI project page](https://pypi.org/project/statementkit/) · binding README in the repo |
| Node | `npm install statementkit` | TypeScript types ship in the package (`lib/index.d.ts`) |
| Java | `io.github.heshiming:statementkit` | javadoc in the jar sources; [Maven Central](https://central.sonatype.com/artifact/io.github.heshiming/statementkit) |
| Go | cgo; module `statementkit/bindings/go` in the repo | godoc comments in `statementkit.go`; [pkg.go.dev](https://pkg.go.dev/) once the public module path is cut |
| C | link `libstatementkit_c` | `crates/statementkit-c/include/statementkit.h` |

Repository: [github.com/heshiming/statementkit](https://github.com/heshiming/statementkit)

Where a binding README and this site disagree, the vectors decide:
`tests/vectors/` is the ground truth for behavior.
