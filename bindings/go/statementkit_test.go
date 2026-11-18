// Shared-vector test: bindings/go must agree with tests/vectors.
package statementkit

import (
	"errors"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

func vectorsPath(t *testing.T) string {
	t.Helper()
	p, err := filepath.Abs(filepath.Join("..", "..", "tests", "vectors", "parse_float.tsv"))
	if err != nil {
		t.Fatal(err)
	}
	return p
}

func TestParseFloatVectors(t *testing.T) {
	data, err := os.ReadFile(vectorsPath(t))
	if err != nil {
		t.Fatal(err)
	}
	rows := 0
	for _, line := range strings.Split(string(data), "\n") {
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		parts := strings.Split(line, "	")
		caseName, input, expect := parts[0], "", ""
		if len(parts) > 1 {
			input = parts[1]
		}
		if len(parts) > 2 {
			expect = parts[2]
		}
		if caseName == "null" {
			continue // Go strings cannot be null; not representable
		}
		rows++
		t.Run(caseName+"/"+input, func(t *testing.T) {
			got, err := ParseFloat(input)
			switch caseName {
			case "ok":
				want, _ := strconv.ParseFloat(expect, 64)
				if err != nil || got != want {
					t.Fatalf("ParseFloat(%q) = %v, %v; want %v", input, got, err, want)
				}
			case "empty":
				if !errors.Is(err, ErrEmpty) {
					t.Fatalf("ParseFloat(%q) err = %v; want ErrEmpty", input, err)
				}
			case "invalid":
				if !errors.Is(err, ErrInvalid) {
					t.Fatalf("ParseFloat(%q) err = %v; want ErrInvalid", input, err)
				}
			}
		})
	}
	if rows < 20 {
		t.Fatalf("only %d vector rows applied", rows)
	}
}

func TestVersion(t *testing.T) {
	v := Version()
	if len(v) < 5 || strings.Count(v, ".") != 2 {
		t.Fatalf("Version() = %q", v)
	}
}
