// Gate 5 variant: does the discipline catch a MISTAKE, or only express the
// happy path? A hand-written state machine that a human wrote correctly is
// not evidence of enforcement -- the question is whether a plausible wrong
// version gets caught.
package main

import (
	"errors"
	"fmt"
	"os"
)

var emissions int

func protect(failAck bool) (string, error) {
	emissions++
	if failAck {
		return "", errors.New("ack lost")
	}
	return "receipt", nil
}

type permit struct{ used bool }

// correctRetry: the disciplined version, refuses to re-initiate.
func correctRetry(failAck bool) {
	p := &permit{}
	if _, err := tryOnce(p, failAck); err != nil {
		for i := 0; i < 2; i++ {
			tryOnce(p, failAck) // turned away: permit spent
		}
	}
}

func tryOnce(p *permit, failAck bool) (string, error) {
	if p.used {
		return "", errors.New("no right")
	}
	p.used = true
	return protect(failAck)
}

// buggyRetry: the plausible mistake -- a developer returns the permit on a
// post-initiation timeout because "nothing succeeded". This is exactly the
// counterfactual gate 4 names, written as a real program.
func buggyRetry(failAck bool) {
	p := &permit{}
	if _, err := tryOnceBuggy(p, failAck); err != nil {
		for i := 0; i < 2; i++ {
			tryOnceBuggy(p, failAck)
		}
	}
}

func tryOnceBuggy(p *permit, failAck bool) (string, error) {
	if p.used {
		return "", errors.New("no right")
	}
	p.used = true
	_, err := protect(failAck)
	if err != nil {
		p.used = false // THE BUG: "it failed, so give the right back"
		return "", err
	}
	return "receipt", nil
}

func main() {
	emissions = 0
	correctRetry(true)
	correct := emissions

	emissions = 0
	buggyRetry(true)
	buggy := emissions

	fmt.Printf("correct=%d buggy=%d\n", correct, buggy)

	// The point of this variant: the hand-written control can be CORRECT by
	// author discipline alone. Nothing in Go stops the buggy version. That is
	// the gap the proposal targets -- and it is an ENFORCEMENT gap, not an
	// expressiveness gap.
	if correct == 1 && buggy == 3 {
		fmt.Println("CONFIRMED: the discipline is expressible by hand; only")
		fmt.Println("           enforcement distinguishes correct from buggy.")
		fmt.Println("GATE5-TRACE-HALF: proposal targets enforcement, not expressiveness")
	} else {
		fmt.Println("UNEXPECTED")
		os.Exit(1)
	}
}
