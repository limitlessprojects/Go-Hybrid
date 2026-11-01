package durable

import (
	"errors"
	"testing"
)

func runUninterrupted(init RegionSnapshot) (RegionSnapshot, int) {
	s, steps := CloneSnapshot(init), 0
	for StepOnce(&s) {
		steps++
	}
	s.Digest = ComputeDigest(s)
	return s, steps
}

func runWithCrashAndRecover(init RegionSnapshot, ckptEvery, crashAtStep int, crashMidCommit bool) (RegionSnapshot, int, int, error) {
	var store AtomicSnapshotStore
	s := CloneSnapshot(init)
	_ = store.CommitAtomic(s, false)
	preSteps := 0
	for preSteps < crashAtStep && StepOnce(&s) {
		preSteps++
		if preSteps%ckptEvery == 0 {
			s.CommitSeq++
			if err := store.CommitAtomic(s, crashMidCommit && preSteps == crashAtStep); err != nil {
				break
			}
		}
	}
	recovered, err := store.LoadLatest()
	if err != nil {
		return RegionSnapshot{}, 0, 0, err
	}
	postSteps := 0
	for StepOnce(recovered) {
		postSteps++
		if postSteps%ckptEvery == 0 || recovered.Completed {
			recovered.CommitSeq++
			if err := store.CommitAtomic(*recovered, false); err != nil {
				return RegionSnapshot{}, 0, 0, err
			}
		}
	}
	return *recovered, preSteps, postSteps, nil
}

func TestExhaustivePTRAndFanOutCrashRecoveryParity(t *testing.T) {
	ptrInit := NewPTRRegion(12, 3)
	ptrRef, ptrTotal := runUninterrupted(ptrInit)
	for crashStep := 1; crashStep <= ptrTotal; crashStep++ {
		for _, mid := range []bool{false, true} {
			got, _, post, err := runWithCrashAndRecover(ptrInit, 3, crashStep, mid)
			if err != nil || !got.Completed || got.Result != ptrRef.Result || (crashStep >= 6 && !mid && post >= ptrTotal) {
				t.Fatalf("PTR step=%d mid=%v err=%v got=%d want=%d post=%d", crashStep, mid, err, got.Result, ptrRef.Result, post)
			}
		}
	}
	foInit := NewFanOutRegion(12, 3, 4)
	foRef, foTotal := runUninterrupted(foInit)
	for crashStep := 1; crashStep <= foTotal; crashStep++ {
		if got, _, _, err := runWithCrashAndRecover(foInit, 4, crashStep, false); err != nil || !got.Completed || got.Result != foRef.Result {
			t.Fatalf("FanOut step=%d err=%v got=%d want=%d", crashStep, err, got.Result, foRef.Result)
		}
	}
}

func TestNegativeControlsF1ToF5(t *testing.T) {
	s := NewPTRRegion(8, 4)
	for i := 0; i < 5; i++ {
		StepOnce(&s)
	}
	s.Digest = ComputeDigest(s)
	f1, f2, f3, f4 := CloneSnapshot(s), CloneSnapshot(s), CloneSnapshot(s), CloneSnapshot(s)
	q := f1.Queues["raw_q"]
	q.Items = q.Items[1:]
	f1.Queues["raw_q"] = q
	f1.Digest = ComputeDigest(f1)
	f2.Tasks[0].Locals["emitted"]++
	f2.Digest = ComputeDigest(f2)
	f3.Tasks[1].PC = 99
	f3.Digest = ComputeDigest(f3)
	f4.BuildID = "hyb-w2620-build-v0-stale"
	f4.Digest = ComputeDigest(f4)
	if !errors.Is(ValidateSnapshot(f1), ErrTornCut) || !errors.Is(ValidateSnapshot(f2), ErrTornCut) ||
		!errors.Is(ValidateSnapshot(f3), ErrInvalidContinuation) || !errors.Is(ValidateSnapshot(f4), ErrBuildIdentityMismatch) {
		t.Fatal("negative control F1..F4 failed to reject corrupt snapshot")
	}
	var store AtomicSnapshotStore
	_ = store.CommitAtomic(NewPTRRegion(8, 4), false)
	if !errors.Is(store.CommitAtomic(s, true), ErrMidWriteCrash) {
		t.Fatal("F5: expected ErrMidWriteCrash")
	}
	if prev, err := store.LoadLatest(); err != nil || prev.CommitSeq != 0 {
		t.Fatalf("F5: expected intact CommitSeq=0, got err=%v", err)
	}
}
