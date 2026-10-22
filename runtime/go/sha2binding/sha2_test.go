package sha2binding

import (
	"encoding/hex"
	"testing"
)

var vectors = []struct {
	in, want string
}{
	{"", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"},
	{"abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"},
}

func TestSha2Digest256(t *testing.T) {
	for _, v := range vectors {
		got, err := Digest256([]byte(v.in))
		if err != nil {
			t.Fatalf("Digest256(%q): %v", v.in, err)
		}
		if len(got) != 32 {
			t.Fatalf("digest len = %d, want 32", len(got))
		}
		if h := hex.EncodeToString(got); h != v.want {
			t.Fatalf("Digest256(%q) = %s, want %s", v.in, h, v.want)
		}
	}
}
