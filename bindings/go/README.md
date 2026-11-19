# Go binding — implemented (cgo)

`statementkit.go` links the static library through cgo with `${SRCDIR}`-
relative flags, so a plain `go test ./...` works right after
`cargo build --release -p statementkit-c`:

```go
v, err := statementkit.ParseFloat("12.25")  // 12.25, nil
_, err = statementkit.ParseFloat("1,2")     // ErrInvalid
statementkit.Version()                      // "0.1.0"
```

Streaming session over named targets (ticker selection optional):

```go
s, err := statementkit.NewSession([]string{"income", "trades"}, []string{"ABC"})
if err != nil { panic(err) } // unknown target: error carries the ABI message
defer s.Close()
s.Feed(data)
s.EndFile()
text, err := s.Report() // compact JSON; decode with json.Number for exact text
```

Sentinels: `ErrEmpty`, `ErrInvalid`, `ErrPanic`, `ErrNullInput`
(unreachable — Go strings cannot be null).

Test: `go test ./...` (shared vectors; the null vector row is skipped).

Distribution note: consumers need CGO and the static lib present at the
relative path, or a packaging step bundling `libstatementkit_c.a`; the
module path is local until the public repo URL exists.
