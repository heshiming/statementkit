# Java binding — published to Maven Central (Panama FFM, JDK 22+)

Maven: `io.github.heshiming:statementkit` — the jar embeds
`libstatementkit_c` for linux-x86_64, linux-aarch64 and windows-x86_64
and extracts the right one at class-load. No JNI, no codegen.

```java
double v = StatementKit.parseFloat("12.25");   // throws Empty/Invalid/Panic
String lib = StatementKit.version();
```

Streaming session over named targets (ticker selection optional):

```java
try (var s = new StatementKit.Session(new String[] {"income", "trades"},
                                      new String[] {"ABC"})) {
    s.feed(Files.readAllBytes(Path.of("statement.csv")));
    s.endFile();
    String text = s.report(); // compact JSON; numeric literals unquoted
}
```

Library discovery: `$STATEMENTKIT_LIB` (path to the `.so`/`.dll`)
overrides; else the jar-embedded native; else `libstatementkit_c` on
`java.library.path` (absolute path required).

Build the jar (needs JDK 22+, Maven, and staged natives):

```sh
pip install cargo-zigbuild ziglang      # once, for the cross-builds
scripts/stage-java-natives.sh           # -> bindings/java/native/lib/<os>-<arch>/
mvn -f bindings/java/pom.xml package -Dgpg.skip
```

Shared-vector drill — loads the jar's embedded native, no env needed:

```sh
java --enable-native-access=ALL-UNNAMED \
    -cp bindings/java/target/statementkit-0.1.0.jar:bindings/java/target/test-classes \
    org.statementkit.StatementKitTest tests/vectors/parse_float.tsv
```

`null` input throws `IllegalArgumentException` (idiomatic Java for the
ABI's `SK_NULL_INPUT`); the null row in the shared vectors maps to it.
Publish: `mvn -f bindings/java/pom.xml deploy -Dgpg.keyname=...` with the
Central token in `~/.m2/settings.xml` (server id `central`); the release
workflow automates it on tag push. A JDK can live at
`bindings/java/.temurin/` (gitignored). Later: jextract stubs from the
header once the API grows.
