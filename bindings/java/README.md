# Java binding (reserved)

Plan:

1. Primary: Panama FFM API (`java.lang.foreign`, JDK 22+) loading the C
   shared library — no JNI build step, no native toolchain for users.
2. Fallback for older JDKs: JNI crate `crates/statementkit-java`, decided
   only if demand requires it.

Rules: thin shell only — no parsing logic here; core test vectors re-run
through the binding.
