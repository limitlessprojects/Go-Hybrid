// Gate 5 (trace half) for W-3269: compare three shapes by the number of
// protected emissions each produces under a lost acknowledgement.
//
// This is the falsifier the proposal names: "compare tiny admitted programs
// and their emitted Go/Rust traces against it. Include a plain (T, error)
// retry control and an explicit hand-written state-machine control."
//
// Each program attempts one protected operation, loses the ack, then retries
// twice. The counter under test is how many times the operation actually ran.
package main

import (
	"errors"
	"fmt"
	"os"
)

// ---------------------------------------------------------------------------
// The protected operation. It records an emission each time it runs.
// ---------------------------------------------------------------------------

var emissions int

// protect simulates one side effect that CANNOT be undone and whose
// acknowledgement can be lost after it ran.
func protect(failAck bool) (string, error) {
	emissions++ // the effect happened
	if failAck {
		return "", errors.New("ack lost")
	}
	return "receipt", nil
}

// ---------------------------------------------------------------------------
// Shape A: plain (T, error) retry -- the status quo the proposal challenges.
// ---------------------------------------------------------------------------

func plainRetry() {
	for i := 0; i < 3; i++ {
		_, err := protect(true)
		if err != nil {
			continue // ordinary retry
		}
	}
}

// ---------------------------------------------------------------------------
// Shape B: hand-written state machine -- the control that shows the contract
// is expressible without language support.
// ---------------------------------------------------------------------------

type permit struct{ used bool }

type outcome struct {
	kind    string // "known" | "uncertain" | "notstarted"
	pending bool
}

func attempt(p *permit, failAck bool) outcome {
	if p.used {
		// no initiation right: cannot run the operation again
		return outcome{kind: "notstarted"}
	}
	p.used = true
	_, err := protect(failAck)
	if err != nil {
		return outcome{kind: "uncertain", pending: true}
	}
	return outcome{kind: "known"}
}

func handWritten() {
	p := &permit{}
	o := attempt(p, true)
	for i := 0; i < 2; i++ {
		// the retry loop asks the pending token for a new right; the state
		// machine refuses because the permit is spent
		if o.kind == "uncertain" && o.pending {
			o = attempt(p, true) // returns notstarted, runs nothing
		}
	}
}

// ---------------------------------------------------------------------------
// Shape C: the proposed permit discipline, same state machine but with the
// pending token carrying the operation identity and no right.
// ---------------------------------------------------------------------------

type pending struct {
	op     string
	owner  string
	settled bool
}

func proposed() {
	permitSpent := false
	var pend *pending

	try := func() {
		if permitSpent {
			return // no right
		}
		permitSpent = true
		_, err := protect(true)
		if err != nil {
			pend = &pending{op: "op1", owner: "main"}
			return
		}
	}
	try()
	for i := 0; i < 2; i++ {
		if pend != nil && !pend.settled {
			// observe only: cannot initiate again
			continue
		}
	}
}

func main() {
	results := map[string]int{}

	emissions = 0
	plainRetry()
	results["plain_retry"] = emissions

	emissions = 0
	handWritten()
	results["hand_written"] = emissions

	emissions = 0
	proposed()
	results["proposed"] = emissions

	fmt.Printf("plain_retry=%d hand_written=%d proposed=%d\n",
		results["plain_retry"], results["hand_written"], results["proposed"])

	// Gate 5 assertion: the plain retry must re-emit, the two disciplined
	// shapes must not. If hand_written already equals proposed, the proposal's
	// language support adds nothing and this is a NARROW/DECLINE signal.
	switch {
	case results["plain_retry"] != 3:
		fmt.Println("FAIL: plain retry did not re-emit three times")
		os.Exit(1)
	case results["hand_written"] != 1 || results["proposed"] != 1:
		fmt.Println("FAIL: a disciplined shape re-emitted")
		os.Exit(1)
	case results["hand_written"] == results["proposed"]:
		fmt.Println("SIGNAL: hand-written control matches the proposal exactly;")
		fmt.Println("        the language support adds no enforceable contract here.")
		fmt.Println("PASS-WITH-NARROW-SIGNAL")
	default:
		fmt.Println("PASS")
	}
}
