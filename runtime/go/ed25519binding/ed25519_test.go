package ed25519binding

import (
	"crypto/ed25519"
	"testing"
)

func TestEd25519SignAndVerify(t *testing.T) {
	seed := [32]byte{1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32}
	msg := []byte("Hello Go-Hybrid Ed25519 Wasm test message")

	// Derive stdlib Ed25519 keypair for verification reference
	stdPriv := ed25519.NewKeyFromSeed(seed[:])
	stdPub := stdPriv.Public().(ed25519.PublicKey)
	var pubKeyBytes [32]byte
	copy(pubKeyBytes[:], stdPub)

	sig, err := Sign(seed, msg)
	if err != nil {
		t.Fatalf("Sign failed: %v", err)
	}

	if !Verify(pubKeyBytes, msg, sig) {
		t.Fatal("Verify returned false for valid signature")
	}

	// Test invalid message
	if Verify(pubKeyBytes, []byte("tampered message"), sig) {
		t.Fatal("Verify returned true for tampered message")
	}
}
