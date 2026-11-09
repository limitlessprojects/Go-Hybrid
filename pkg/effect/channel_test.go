package effect_test

import (
	"context"
	"errors"
	"testing"
	"time"

	"limitlessprojects/Go-Hybrid/pkg/effect"
)

func TestTypestatedChannelLifecycle(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	tx, rx := effect.NewChannel[int](ctx, 2)

	if err := tx.Send(ctx, 42); err != nil {
		t.Fatalf("unexpected send error: %v", err)
	}

	val, ok, err := rx.Recv(ctx)
	if err != nil || !ok || val != 42 {
		t.Fatalf("recv failed, got val=%d, ok=%v, err=%v", val, ok, err)
	}

	if err := tx.Close(); err != nil {
		t.Fatalf("unexpected close error: %v", err)
	}

	if err := tx.Close(); !errors.Is(err, effect.ErrAlreadyClosed) {
		t.Fatalf("expected ErrAlreadyClosed, got: %v", err)
	}

	if err := tx.Send(ctx, 100); !errors.Is(err, effect.ErrChannelClosed) {
		t.Fatalf("expected ErrChannelClosed, got: %v", err)
	}
}

func TestNurseryCancellationCascade(t *testing.T) {
	parentCtx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	nursery, nCtx := effect.NewNursery(parentCtx)
	tx, rx := effect.NewChannel[string](nCtx, 0)

	testErr := errors.New("task failure")

	nursery.Start(func(ctx context.Context) error {
		return testErr
	})

	nursery.Start(func(ctx context.Context) error {
		// Try sending on synchronous unbuffered channel; should abort via nursery context cancel
		err := tx.Send(ctx, "hello")
		if err != nil && !errors.Is(err, effect.ErrNurseryCanceled) && !errors.Is(err, context.Canceled) {
			return err
		}
		return nil
	})

	err := nursery.Wait()
	if !errors.Is(err, testErr) {
		t.Fatalf("expected nursery to return testErr, got: %v", err)
	}

	_ = rx
}
