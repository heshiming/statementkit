# Go

cgo over the static library. In the repository, a plain
`go test ./...` works right after `cargo build --release -p
statementkit-c` — the cgo flags are `${SRCDIR}`-relative.

## Scalars

```go
v, err := statementkit.ParseFloat("12.25")  // 12.25, nil
_, err = statementkit.ParseFloat("1,2")     // ErrInvalid
statementkit.Version()                      // "0.2.0"
```

Sentinels: `ErrEmpty`, `ErrInvalid`, `ErrPanic`, `ErrNullInput`
(the last is unreachable — Go strings cannot be null).

## Streaming session

```go
s, err := statementkit.NewSession([]string{"income", "trades"},
                                  []string{"ABC"})
if err != nil { panic(err) } // unknown target: error carries the ABI message
defer s.Close()
s.Feed(data)
s.EndFile()
text, err := s.Report() // compact JSON; decode with json.Number for exact text
```

The ticker selection may be nil; the selected form's report carries a
`filter` fragment.

## Distribution note

Consumers need CGO enabled and the static library present at the
relative path, or a packaging step that bundles
`libstatementkit_c.a`. The module path is in-repo until the public
module cut; see [API references](../references.md).
