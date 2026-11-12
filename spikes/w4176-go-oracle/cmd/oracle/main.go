// Command oracle runs the real Go binary (built by `hyb build --race`) many
// times under several GOMAXPROCS settings and records whether the race
// detector or the runtime fired. This is the one-sided falsifier for the
// certificate: CERTIFIED must never fire; RACE_DETECTED must fire.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

type Result struct {
	Program string `json:"program"`
	Status  string `json:"status"`
	Bin     string `json:"bin"`
	Runs    int    `json:"runs"`
	Races   int    `json:"races"`
	Fatals  int    `json:"fatals"`
	NonZero int    `json:"nonzero"`
	Sample  string `json:"sample,omitempty"`
	Agrees  bool   `json:"agrees"`
}

func main() {
	results := flag.String("results", "results.json", "corpus results json")
	binDir := flag.String("bin", "", "directory of race-enabled binaries (<program>.bin)")
	runs := flag.Int("runs", 20, "runs per GOMAXPROCS setting")
	out := flag.String("out", "oracle.json", "output json")
	flag.Parse()
	if *binDir == "" {
		fmt.Fprintln(os.Stderr, "need -bin")
		os.Exit(2)
	}
	var rows []struct {
		Program string `json:"program"`
		Status  string `json:"status"`
	}
	b, err := os.ReadFile(*results)
	if err != nil {
		fatal(err)
	}
	if err := json.Unmarshal(b, &rows); err != nil {
		fatal(err)
	}
	var res []Result
	for _, r := range rows {
		if r.Status != "CERTIFIED" && r.Status != "RACE_DETECTED" {
			continue
		}
		bin := filepath.Join(*binDir, r.Program+".bin")
		if _, err := os.Stat(bin); err != nil {
			res = append(res, Result{Program: r.Program, Status: r.Status, Bin: bin, Sample: "binary missing"})
			continue
		}
		rr := Result{Program: r.Program, Status: r.Status, Bin: bin}
		for _, procs := range []string{"1", "4", "8"} {
			for i := 0; i < *runs; i++ {
				cmd := exec.Command(bin)
				cmd.Env = append(os.Environ(), "GOMAXPROCS="+procs, "GORACE=atexit_sleep_ms=0")
				done := make(chan struct{})
				var outb []byte
				var runErr error
				go func() {
					outb, runErr = cmd.CombinedOutput()
					close(done)
				}()
				select {
				case <-done:
				case <-time.After(10 * time.Second):
					cmd.Process.Kill()
					<-done
					rr.Fatals++
					rr.Sample = "timeout"
					rr.Runs++
					continue
				}
				rr.Runs++
				out := string(outb)
				if strings.Contains(out, "WARNING: DATA RACE") {
					rr.Races++
					if rr.Sample == "" {
						rr.Sample = firstRace(out)
					}
				}
				if strings.Contains(out, "concurrent map writes") || strings.Contains(out, "fatal error:") {
					rr.Fatals++
					if rr.Sample == "" {
						rr.Sample = "fatal: concurrent map writes"
					}
				}
				if runErr != nil {
					rr.NonZero++
				}
			}
		}
		fired := rr.Races > 0 || rr.Fatals > 0
		if r.Status == "CERTIFIED" {
			rr.Agrees = !fired
		} else {
			rr.Agrees = fired
		}
		res = append(res, rr)
		fmt.Printf("%-42s %-14s runs=%d races=%d fatals=%d agree=%v\n", rr.Program, rr.Status, rr.Runs, rr.Races, rr.Fatals, rr.Agrees)
	}
	b, _ = json.MarshalIndent(res, "", " ")
	os.WriteFile(*out, append(b, '\n'), 0o644)

	certified, raceDetected, agree, disagree := 0, 0, 0, 0
	for _, r := range res {
		switch r.Status {
		case "CERTIFIED":
			certified++
		case "RACE_DETECTED":
			raceDetected++
		}
		if r.Agrees {
			agree++
		} else {
			disagree++
			fmt.Printf("DISAGREE %s status=%s races=%d fatals=%d\n", r.Program, r.Status, r.Races, r.Fatals)
		}
	}
	fmt.Printf("oracle: certified=%d race_detected=%d agree=%d disagree=%d\n", certified, raceDetected, agree, disagree)
}

func firstRace(out string) string {
	lines := strings.Split(out, "\n")
	for i, l := range lines {
		if strings.Contains(l, "WARNING: DATA RACE") && i+3 < len(lines) {
			return strings.TrimSpace(lines[i+1]) + " | " + strings.TrimSpace(lines[i+2])
		}
	}
	return "race"
}

func fatal(err error) {
	fmt.Fprintln(os.Stderr, "error:", err)
	os.Exit(1)
}
