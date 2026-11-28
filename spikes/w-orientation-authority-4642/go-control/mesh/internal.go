package mesh

// insertEpochOnly is the measurement-only variant: same identity/epoch/triple
// checks, no coordinate digest. Unexported on purpose -- it is not an API a
// caller may use, it exists to price the digest in the benchmark.
func (m *Mesh) insertEpochOnly(t Triple, w Turn) (int8, error) {
	if w.cap != theMint {
		return 0, ErrForged
	}
	if w.mesh != m {
		return 0, ErrMesh
	}
	if w.epoch != m.epoch {
		return 0, ErrEpoch
	}
	if w.t != t {
		return 0, ErrTriple
	}
	m.epoch++
	return w.dir, nil
}
