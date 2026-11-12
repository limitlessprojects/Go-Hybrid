package sha3binding

import (
	"bytes"
	"encoding/hex"
	"testing"
)

// NIST SHA3-256 vectors — the pinned crate (sha3 0.10.8) through the real
// bridge. Example 10's acceptance already proved the digest path once per
// run; these pin the vectors permanently in go test.

var vectors = []struct {
	in, want string
}{
	{"", "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a"},
	{"abc", "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"},
}

func TestDigest256NISTVectors(t *testing.T) {
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

func TestDigest256EmptyInputNoError(t *testing.T) {
	// len == 0 with a nil slice: the null-guard contract (len>0 && data
	// null is the error; empty is legal).
	got, err := Digest256(nil)
	if err != nil {
		t.Fatalf("Digest256(nil): %v", err)
	}
	if want, _ := hex.DecodeString(vectors[0].want); !bytes.Equal(got, want) {
		t.Fatal("nil-input digest != empty-input digest")
	}
}

func TestStreamChecksumDeterministic(t *testing.T) {
	a1, err := StreamChecksum(1000, 42)
	if err != nil {
		t.Fatalf("StreamChecksum: %v", err)
	}
	a2, err := StreamChecksum(1000, 42)
	if err != nil {
		t.Fatalf("StreamChecksum: %v", err)
	}
	if a1 != a2 {
		t.Fatalf("checksum differs across identical calls: %d vs %d", a1, a2)
	}
	b, _ := StreamChecksum(1000, 43)
	if a1 == b {
		t.Fatal("different seeds produced identical checksums")
	}
	c, _ := StreamChecksum(2000, 42)
	if a1 == c {
		t.Fatal("different iteration counts produced identical checksums")
	}
}

func TestDigest256BorrowedMatchesPinned(t *testing.T) {
	for _, n := range []int{0, 1, 1 << 10, 64 << 10} {
		buf := make([]byte, n)
		for i := range buf {
			buf[i] = byte(i * 7)
		}
		want, err := Digest256(buf)
		if err != nil {
			t.Fatal(err)
		}
		got, err := Digest256Borrowed(buf)
		if err != nil {
			t.Fatal(err)
		}
		if !bytes.Equal(got, want) {
			t.Fatalf("n=%d: borrowed digest differs from pinned digest", n)
		}
	}
}
