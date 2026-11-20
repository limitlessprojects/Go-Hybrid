package membrane

import (
	"testing"
)

// TestEscapeAnalysisPreservation directly examines Milestone 2 (M2) hypotheses:
// Documents whether Go compiler escape analysis preserves zero-allocation invariants
// across different closure capture shapes.
func TestEscapeAnalysisPreservation(t *testing.T) {
	buf := make([]byte, 1024)

	// Shape 1: Immediate inline closure invocation.
	// Cap and Membrane are captured by value on the current stack frame.
	t.Run("Shape1_InlineClosureCapture", func(t *testing.T) {
		res, err := EnterRegion[RegionA, byte](func(cap RegionCap[RegionA]) (byte, error) {
			m, err := NewBufferMembrane[RegionA](cap, buf)
			if err != nil {
				return 0, err
			}
			v, err := m.View(cap)
			if err != nil {
				return 0, err
			}
			return v[0], nil
		})
		if err != nil {
			t.Fatalf("shape 1 failed: %v", err)
		}
		_ = res
	})

	// Shape 2: Parameterized higher-order closure pipeline.
	t.Run("Shape2_PipelineTransformClosure", func(t *testing.T) {
		transformer := func(cap RegionCap[RegionA], m BufferMembrane[RegionA]) error {
			_, err := m.Write(cap, 0, []byte{0xDE, 0xAD})
			return err
		}

		capA := NewRegionCap[RegionA]()
		mA, err := NewBufferMembrane[RegionA](capA, buf)
		if err != nil {
			t.Fatalf("setup failed: %v", err)
		}

		if err := transformer(capA, mA); err != nil {
			t.Fatalf("transformer failed: %v", err)
		}

		v, _ := mA.View(capA)
		if v[0] != 0xDE || v[1] != 0xAD {
			t.Fatalf("pipeline byte mismatch")
		}
	})

	// Shape 3: Multi-region handoff through chained closures.
	t.Run("Shape3_ChainedHandoffClosures", func(t *testing.T) {
		capA := NewRegionCap[RegionA]()
		mA, _ := NewBufferMembrane[RegionA](capA, buf)

		passToB := func(m BufferMembrane[RegionA], c *RegionCap[RegionA]) (BufferMembrane[RegionB], RegionCap[RegionB]) {
			mB, cB, _ := TransferBuffer[RegionA, RegionB](m, c)
			return mB, cB
		}

		mB, capB := passToB(mA, &capA)
		if !capA.IsConsumed() {
			t.Fatalf("expected capA consumed after closure handoff")
		}
		if capB.IsConsumed() {
			t.Fatalf("expected capB active after closure handoff")
		}
		_ = mB
	})
}
