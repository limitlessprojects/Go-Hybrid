package analyzer

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
)

const hybridSnapshotEnv = "HYBRID_W012_SNAPSHOT"

// HybridSnapshot is the immutable input contract used by the supported
// `hybvet vet` wrapper. Keys are absolute cleaned .hy paths and values are
// SHA-256 digests of the bytes captured before go vet starts.
type HybridSnapshot struct {
	Version int               `json:"version"`
	Root    string            `json:"root"`
	Files   map[string]string `json:"files"`
}

func CaptureHybridSnapshot(root string) (*HybridSnapshot, error) {
	absRoot, err := filepath.Abs(root)
	if err != nil {
		return nil, err
	}
	absRoot = filepath.Clean(absRoot)
	s := &HybridSnapshot{Version: 1, Root: absRoot, Files: make(map[string]string)}

	err = filepath.WalkDir(absRoot, func(path string, d fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if d.IsDir() {
			switch d.Name() {
			case ".git", "target", "node_modules":
				if path != absRoot {
					return filepath.SkipDir
				}
			}
			return nil
		}
		if filepath.Ext(d.Name()) != ".hy" {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		abs, err := filepath.Abs(path)
		if err != nil {
			return err
		}
		s.Files[filepath.Clean(abs)] = digestBytes(data)
		return nil
	})
	if err != nil {
		return nil, err
	}
	return s, nil
}

func (s *HybridSnapshot) Encode() (string, error) {
	if s == nil || s.Version != 1 || s.Root == "" || s.Files == nil {
		return "", errors.New("invalid hybrid snapshot")
	}
	data, err := json.Marshal(s)
	if err != nil {
		return "", err
	}
	return string(data), nil
}

func hybridSnapshotFromEnv() (*HybridSnapshot, error) {
	raw := os.Getenv(hybridSnapshotEnv)
	if raw == "" {
		return nil, nil
	}
	var s HybridSnapshot
	if err := json.Unmarshal([]byte(raw), &s); err != nil {
		return nil, fmt.Errorf("decode %s: %w", hybridSnapshotEnv, err)
	}
	if s.Version != 1 || s.Root == "" || s.Files == nil {
		return nil, fmt.Errorf("unsupported or incomplete snapshot")
	}
	return &s, nil
}

func (s *HybridSnapshot) VerifyFile(path string, data []byte) error {
	if s == nil {
		return nil
	}
	abs, err := filepath.Abs(path)
	if err != nil {
		return err
	}
	abs = filepath.Clean(abs)
	want, ok := s.Files[abs]
	if !ok {
		return fmt.Errorf("%s was not present in the pre-analysis .hy snapshot", abs)
	}
	got := digestBytes(data)
	if got != want {
		return fmt.Errorf("%s changed after the pre-analysis .hy snapshot", abs)
	}
	return nil
}

// VerifyCurrent detects mutation, deletion, or addition anywhere under the
// captured workspace root. This is intentionally conservative: a concurrent
// .hy edit makes the wrapper fail closed rather than certify mixed inputs.
func (s *HybridSnapshot) VerifyCurrent() error {
	if s == nil {
		return errors.New("nil hybrid snapshot")
	}
	current, err := CaptureHybridSnapshot(s.Root)
	if err != nil {
		return err
	}
	if !reflect.DeepEqual(s.Files, current.Files) {
		return errors.New(".hy workspace changed while analysis was running")
	}
	return nil
}

func digestBytes(data []byte) string {
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:])
}
