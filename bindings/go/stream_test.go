package statementkit

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

type streamVector struct {
	Targets []string `json:"targets"`
	Tickers []string `json:"tickers"`
	Files   []string `json:"files"`
	Expect  []string `json:"expect"`
}

func runStreamVector(t *testing.T, name string) {
	t.Helper()
	dir := filepath.Join("..", "..", "tests", "vectors")
	raw, err := os.ReadFile(filepath.Join(dir, name))
	if err != nil {
		t.Fatal(err)
	}
	var v streamVector
	if err := json.Unmarshal(raw, &v); err != nil {
		t.Fatal(err)
	}
	s, err := NewSession(v.Targets, v.Tickers)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	for _, f := range v.Files {
		data, err := os.ReadFile(filepath.Join(dir, f))
		if err != nil {
			t.Fatal(err)
		}
		if err := s.Feed(data); err != nil {
			t.Fatal(err)
		}
		if err := s.EndFile(); err != nil {
			t.Fatal(err)
		}
	}
	text, err := s.Report()
	if err != nil {
		t.Fatal(err)
	}
	for _, frag := range v.Expect {
		if !strings.Contains(text, frag) {
			t.Fatalf("%s: missing fragment %q", name, frag[:min(60, len(frag))])
		}
	}
}

func TestStreamVector(t *testing.T)     { runStreamVector(t, "stream.json") }
func TestStreamFilterVector(t *testing.T) { runStreamVector(t, "stream_filter.json") }

func TestUnknownTargetMessage(t *testing.T) {
	_, err := NewSession([]string{"nope"}, nil)
	if err == nil || !strings.Contains(err.Error(), "unknown targets: nope") {
		t.Fatalf("want ABI message, got %v", err)
	}
}
