# Java binding — implemented (Panama FFM, JDK 22+)

`src/org/statementkit/StatementKit.java` uses `java.lang.foreign` downcalls
against the C ABI — no JNI, no codegen.

```java
double v = StatementKit.parseFloat("12.25");   // throws Empty/Invalid/Panic
String lib = StatementKit.version();
```

Library discovery: `$STATEMENTKIT_LIB` (path to the `.so`/`.dylib`/`.dll`)
or `libstatementkit_c` on `java.library.path` (absolute path required).

Test + compile (JDK 25 locally; any 22+ works):

```sh
bindings/java/.temurin/bin/javac -d bindings/java/out \
    bindings/java/src/org/statementkit/*.java
STATEMENTKIT_LIB=$PWD/target/release/libstatementkit_c.so \
    bindings/java/.temurin/bin/java --enable-native-access=ALL-UNNAMED \
    -cp bindings/java/out org.statementkit.StatementKitTest \
    tests/vectors/parse_float.tsv
```

`null` input throws `IllegalArgumentException` (idiomatic Java for the
ABI's `SK_NULL_INPUT`); the null row in the shared vectors maps to it. A
JDK can live at `bindings/java/.temurin/` (gitignored). Later: jextract
stubs from the header once the API grows.
