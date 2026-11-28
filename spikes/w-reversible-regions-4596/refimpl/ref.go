// Package refimpl is the independent reference: hand-written inverse transforms,
// derived by reading each case's forward statements and inverting them by hand.
//
// It deliberately shares no code with package ir: different state type, no IR
// walk, and the S-box inverse is derived algebraically (modular inverse of the
// affine multiplier) rather than by inverting the table.
package refimpl

import "math/bits"

// S is the reference state (8 words; the key is passed separately).
type S struct {
	A0, A1, A2, A3, A4, A5, A6, A7 uint64
}

// ---- byte-wise affine table, both directions, derived independently ----

func affineForward(v uint64, a, b byte) uint64 {
	var out uint64
	for i := 0; i < 8; i++ {
		x := byte(v >> (8 * uint(i)))
		out |= uint64(byte(uint16(x)*uint16(a))^b) << (8 * uint(i))
	}
	return out
}

// affineInverse inverts y = (x*a mod 256) ^ b by solving x = ((y^b) * a^-1) mod 256,
// where a^-1 is found by brute force over the multiplicative group mod 256.
func affineInverse(v uint64, a, b byte) uint64 {
	var ainv byte
	for i := 0; i < 256; i++ {
		if byte(uint16(i)*uint16(a)) == 1 {
			ainv = byte(i)
			break
		}
	}
	var out uint64
	for i := 0; i < 8; i++ {
		y := byte(v >> (8 * uint(i)))
		x := byte(uint16(y^b) * uint16(ainv))
		out |= uint64(x) << (8 * uint(i))
	}
	return out
}

const (
	s1A, s1B = 0x1d, 0x63 // pos_sbox_bytes
	s2A, s2B = 0x8d, 0x2f // pos_full_mix_branch
)

// InvPosRotlXorKey inverts: x0 = rotl(x0,7); x0 ^= key.
func InvPosRotlXorKey(s S, key uint64) S {
	s.A0 ^= key
	s.A0 = bits.RotateLeft64(s.A0, -7)
	return s
}

// InvPosSwapPair inverts: swap x0,x1; swap x2,x3; x4 ^= C. Reverse order is required.
func InvPosSwapPair(s S, key uint64) S {
	s.A4 ^= 0x00ff00ff00ff00ff
	s.A2, s.A3 = s.A3, s.A2
	s.A0, s.A1 = s.A1, s.A0
	return s
}

// InvPosArxRound inverts: x0+=x1; x1=rotl(x1,13); x0^=x1; x2^=0xa5; x2-=7; x3+=x2.
func InvPosArxRound(s S, key uint64) S {
	s.A3 -= s.A2
	s.A2 += 7
	s.A2 ^= 0xa5
	s.A0 ^= s.A1
	s.A1 = bits.RotateLeft64(s.A1, -13)
	s.A0 -= s.A1
	return s
}

// applyPerm applies new[i] = old[p[i]] using an explicit pre-image array.
func applyPerm(s S, p [8]int) S {
	in := [8]uint64{s.A0, s.A1, s.A2, s.A3, s.A4, s.A5, s.A6, s.A7}
	var out [8]uint64
	for i := 0; i < 8; i++ {
		out[i] = in[p[i]]
	}
	return S{A0: out[0], A1: out[1], A2: out[2], A3: out[3], A4: out[4], A5: out[5], A6: out[6], A7: out[7]}
}

// InvPosPermCycle inverts: perm(2 0 3 1 5 4 7 6); x4^=0x1234; x6+=x7.
func InvPosPermCycle(s S, key uint64) S {
	s.A6 -= s.A7
	s.A4 ^= 0x1234
	// inverse of p = 2 0 3 1 5 4 7 6 is q = 1 3 0 2 5 4 7 6 (out[i] = in[q[i]])
	s = applyPerm(s, [8]int{1, 3, 0, 2, 5, 4, 7, 6})
	return s
}

// InvPosBranchAddsub inverts: if x7.bit0==0 { x0+=K } else { x1-=3 }.
func InvPosBranchAddsub(s S, key uint64) S {
	if (s.A7>>0)&1 == 0 {
		s.A0 -= 0x9e3779b97f4a7c15
	} else {
		s.A1 += 3
	}
	return s
}

// InvPosBranchNested inverts the nested conditional; x6 is never written so the
// same tests re-select the same arms.
func InvPosBranchNested(s S, key uint64) S {
	if (s.A6>>3)&1 == 1 {
		s.A0 ^= s.A1
		if (s.A6>>5)&1 == 0 {
			s.A1 ^= 0xdeadbeef
		} else {
			s.A2 -= 0xcafe
		}
		s.A0 = bits.RotateLeft64(s.A0, -9)
	} else {
		s.A2 = bits.RotateLeft64(s.A2, -17)
		s.A0 += s.A2
	}
	return s
}

// InvPosBranchXorKey inverts a branch discriminated by a key bit.
func InvPosBranchXorKey(s S, key uint64) S {
	if (key>>4)&1 == 1 {
		s.A0 = bits.RotateLeft64(s.A0, -11)
		s.A0 ^= 0x5555aaaa5555aaaa
	} else {
		s.A2, s.A3 = s.A3, s.A2
		s.A0 -= s.A2
	}
	return s
}

// InvPosWideMix inverts the 9-statement all-register mix, strictly reversed.
func InvPosWideMix(s S, key uint64) S {
	s.A0, s.A6 = s.A6, s.A0
	s.A5 ^= 0xdeadbeefcafebabe
	s.A7 = bits.RotateLeft64(s.A7, 40)
	s.A6 ^= s.A7
	s.A4 -= s.A5
	s.A3 += 3
	s.A2 = bits.RotateLeft64(s.A2, -25)
	s.A1 ^= s.A0
	s.A0 -= 0x9e3779b97f4a7c15
	return s
}

// InvPosSboxBytes inverts: sbox(x0); rotl(x3,7); sbox(x3); x0^=x3.
func InvPosSboxBytes(s S, key uint64) S {
	s.A0 ^= s.A3
	s.A3 = affineInverse(s.A3, s1A, s1B)
	s.A3 = bits.RotateLeft64(s.A3, -7)
	s.A0 = affineInverse(s.A0, s1A, s1B)
	return s
}

// InvPosFullMixBranch inverts perm + sbox + add + branch.
func InvPosFullMixBranch(s S, key uint64) S {
	if (s.A5>>2)&1 == 0 {
		s.A6 = affineInverse(s.A6, s2A, s2B)
		s.A4 ^= s.A6
	} else {
		s.A7 += 5
		s.A6 = bits.RotateLeft64(s.A6, -19)
	}
	s.A4 -= 0x0123456789abcdef
	s.A0 = affineInverse(s.A0, s2A, s2B)
	// inverse of p = 3 0 2 1 6 7 4 5 is q = 1 3 2 0 6 7 4 5
	s = applyPerm(s, [8]int{1, 3, 2, 0, 6, 7, 4, 5})
	return s
}

// Forward counterparts, also hand-written, so the reference can be checked
// against the emitted code in both directions rather than only via round-trip.
func ForwardPosRotlXorKey(s S, key uint64) S {
	s.A0 = bits.RotateLeft64(s.A0, 7)
	s.A0 ^= key
	return s
}

func ForwardPosArxRound(s S, key uint64) S {
	s.A0 += s.A1
	s.A1 = bits.RotateLeft64(s.A1, 13)
	s.A0 ^= s.A1
	s.A2 ^= 0xa5
	s.A2 -= 7
	s.A3 += s.A2
	return s
}

func ForwardPosBranchNested(s S, key uint64) S {
	if (s.A6>>3)&1 == 1 {
		s.A0 = bits.RotateLeft64(s.A0, 9)
		if (s.A6>>5)&1 == 0 {
			s.A1 ^= 0xdeadbeef
		} else {
			s.A2 += 0xcafe
		}
		s.A0 ^= s.A1
	} else {
		s.A0 -= s.A2
		s.A2 = bits.RotateLeft64(s.A2, 17)
	}
	return s
}

func ForwardPosSboxBytes(s S, key uint64) S {
	s.A0 = affineForward(s.A0, s1A, s1B)
	s.A3 = bits.RotateLeft64(s.A3, 7)
	s.A3 = affineForward(s.A3, s1A, s1B)
	s.A0 ^= s.A3
	return s
}

func ForwardPosFullMixBranch(s S, key uint64) S {
	s = applyPerm(s, [8]int{3, 0, 2, 1, 6, 7, 4, 5})
	s.A0 = affineForward(s.A0, s2A, s2B)
	s.A4 += 0x0123456789abcdef
	if (s.A5>>2)&1 == 0 {
		s.A4 ^= s.A6
		s.A6 = affineForward(s.A6, s2A, s2B)
	} else {
		s.A6 = bits.RotateLeft64(s.A6, 19)
		s.A7 -= 5
	}
	return s
}

func ForwardPosWideMix(s S, key uint64) S {
	s.A0 += 0x9e3779b97f4a7c15
	s.A1 ^= s.A0
	s.A2 = bits.RotateLeft64(s.A2, 25)
	s.A3 -= 3
	s.A4 += s.A5
	s.A6 ^= s.A7
	s.A7 = bits.RotateLeft64(s.A7, -40)
	s.A5 ^= 0xdeadbeefcafebabe
	s.A0, s.A6 = s.A6, s.A0
	return s
}

func ForwardPosSwapPair(s S, key uint64) S {
	s.A0, s.A1 = s.A1, s.A0
	s.A2, s.A3 = s.A3, s.A2
	s.A4 ^= 0x00ff00ff00ff00ff
	return s
}

func ForwardPosPermCycle(s S, key uint64) S {
	s = applyPerm(s, [8]int{2, 0, 3, 1, 5, 4, 7, 6})
	s.A4 ^= 0x1234
	s.A6 += s.A7
	return s
}

func ForwardPosBranchAddsub(s S, key uint64) S {
	if (s.A7>>0)&1 == 0 {
		s.A0 += 0x9e3779b97f4a7c15
	} else {
		s.A1 -= 3
	}
	return s
}

func ForwardPosBranchXorKey(s S, key uint64) S {
	if (key>>4)&1 == 1 {
		s.A0 ^= 0x5555aaaa5555aaaa
		s.A0 = bits.RotateLeft64(s.A0, 11)
	} else {
		s.A0 += s.A2
		s.A2, s.A3 = s.A3, s.A2
	}
	return s
}
