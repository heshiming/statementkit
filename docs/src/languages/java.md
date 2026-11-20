# Java

Maven Central:

```xml
<dependency>
  <groupId>io.github.heshiming</groupId>
  <artifactId>statementkit</artifactId>
  <version>0.2.0</version>
</dependency>
```

Panama FFM, JDK 22+. No JNI, no codegen. The jar embeds
`libstatementkit_c` for linux-x86_64, linux-aarch64 and
windows-x86_64 and extracts the right one at class-load.

## Scalars

```java
double v = StatementKit.parseFloat("12.25");   // throws Empty/Invalid/Panic
String lib = StatementKit.version();
```

`null` input throws `IllegalArgumentException` — the idiomatic Java
answer to the ABI's `SK_NULL_INPUT`.

## Streaming session

```java
try (var s = new StatementKit.Session(new String[] {"income", "trades"},
                                      new String[] {"ABC"})) {
    s.feed(Files.readAllBytes(Path.of("statement.csv")));
    s.endFile();
    String text = s.report(); // compact JSON; numeric literals unquoted
}
```

The ticker array may be omitted for no selection; the selected form's
report carries a `filter` fragment.

## Library discovery

`$STATEMENTKIT_LIB` (path to the `.so`/`.dll`) overrides; else the
jar-embedded native; else `libstatementkit_c` on `java.library.path`
(absolute path required).
