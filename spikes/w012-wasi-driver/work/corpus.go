package main

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

var verdictLine = regexp.MustCompile(`(?m)^//[ \t]*EXPECTED-VERDICT:[ \t]*(ACCEPT-MANAGED|ACCEPT|REJECT)\b`)

// CorpusFile is one pre-registered program with the verdict it must draw.
type CorpusFile struct {
	Path     string
	Expected ExpectedVerdict
}

// LoadCorpus reads every .hy program under dir and its EXPECTED-VERDICT line.
// A missing or unreadable header is an error, not a skip: the corpus is the
// spec, and a fixture without a pre-registered verdict is not comparable.
func LoadCorpus(dir string) ([]CorpusFile, error) {
	dir, err := filepath.Abs(dir)
	if err != nil {
		return nil, fmt.Errorf("resolve corpus dir: %w", err)
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, fmt.Errorf("read corpus: %w", err)
	}
	var files []CorpusFile
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".hy") {
			continue
		}
		path := filepath.Join(dir, e.Name())
		body, err := os.ReadFile(path)
		if err != nil {
			return nil, fmt.Errorf("read %s: %w", path, err)
		}
		cf, err := parseCorpusFile(path, string(body))
		if err != nil {
			return nil, err
		}
		files = append(files, cf)
	}
	sort.Slice(files, func(i, j int) bool { return files[i].Path < files[j].Path })
	if len(files) == 0 {
		return nil, fmt.Errorf("no .hy programs under %s", dir)
	}
	return files, nil
}

func parseCorpusFile(path, body string) (CorpusFile, error) {
	m := verdictLine.FindStringSubmatch(body)
	if m == nil {
		return CorpusFile{}, fmt.Errorf("%s: no // EXPECTED-VERDICT: header", path)
	}
	switch m[1] {
	case "ACCEPT":
		return CorpusFile{Path: path, Expected: ExpectedVerdict{Verdict: "accept"}}, nil
	case "ACCEPT-MANAGED":
		return CorpusFile{Path: path, Expected: ExpectedVerdict{Verdict: "accept", Managed: true}}, nil
	case "REJECT":
		return CorpusFile{Path: path, Expected: ExpectedVerdict{Verdict: "reject"}}, nil
	default:
		return CorpusFile{}, fmt.Errorf("%s: unknown verdict %q", path, m[1])
	}
}

// Workspace picks up to n programs spread across the corpus and copies them
// into a scratch dir, so benchmark edits never touch the repository.
func Workspace(corpus []CorpusFile, dir string, n int) ([]string, error) {
	if n <= 0 || n > len(corpus) {
		return nil, fmt.Errorf("workspace size %d outside 1..%d", n, len(corpus))
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return nil, err
	}
	stride := len(corpus) / n
	if stride < 1 {
		stride = 1
	}
	var paths []string
	for i := 0; len(paths) < n; i += stride {
		src := corpus[i%len(corpus)]
		dst := filepath.Join(dir, filepath.Base(src.Path))
		body, err := os.ReadFile(src.Path)
		if err != nil {
			return nil, err
		}
		if err := os.WriteFile(dst, body, 0o644); err != nil {
			return nil, err
		}
		paths = append(paths, dst)
	}
	return paths, nil
}

// Edit applies one keystroke-scale change: a trailing comment line. It never
// moves an existing line, so a verdict change can only come from the checker,
// not from the edit rewriting a diagnostic position.
func Edit(path string, n int) error {
	f, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()
	_, err = fmt.Fprintf(f, "// keystroke %d\n", n)
	return err
}
