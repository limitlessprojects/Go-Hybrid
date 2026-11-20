// Command hcert emits and replays hcert-v2 certificates over the Go that
// `hyb build --emit-go` produced for a .hy program.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"

	"hybrid.local/w4176gooracle/internal/cert"
	"hybrid.local/w4176gooracle/internal/lts"
	"hybrid.local/w4176gooracle/internal/model"
)

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}
	switch os.Args[1] {
	case "emit":
		cmdEmit(os.Args[2:])
	case "verify":
		cmdVerify(os.Args[2:])
	case "recheck":
		cmdRecheck(os.Args[2:])
	case "corpus":
		cmdCorpus(os.Args[2:])
	default:
		usage()
		os.Exit(2)
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, `usage:
  hcert emit    <emitted.go> -o <cert.json> [--json]
  hcert verify  <cert.json> --go <emitted.go>
  hcert recheck <emitted.go>
  hcert corpus  --corpus <tests/checker-corpus> --go <emitted-go-dir> --out <results.json>`)
}

func sha256Of(path string) string {
	b, err := os.ReadFile(path)
	if err != nil {
		fatal(err)
	}
	return cert.SHA256Bytes(b)
}

func cmdEmit(args []string) {
	pos, flags := splitFlags(args)
	out := flags["o"]
	if len(pos) != 1 || out == "" {
		usage()
		os.Exit(2)
	}
	path := pos[0]
	sha := sha256Of(path)
	p, err := model.Extract(path, sha)
	if err != nil {
		fatal(err)
	}
	if len(p.Outside) > 0 {
		fmt.Printf("OUTSIDE_BOUND %s %v\n", filepath.Base(path), p.Outside)
		return
	}
	m := lts.NewMachine(p)
	t0 := time.Now()
	full, err := m.Enumerate(false, 2000000)
	if err != nil {
		fatal(err)
	}
	gen := time.Since(t0)
	por, err := m.Enumerate(true, 100000)
	if err != nil {
		fatal(err)
	}
	if full.Race != por.Race {
		fmt.Fprintf(os.Stderr, "POR DISAGREES with full enumeration: full=%v por=%v\n", full.Race, por.Race)
		os.Exit(3)
	}
	c := cert.Emit(p, m, full, por)
	if err := c.Save(out); err != nil {
		fatal(err)
	}
	fmt.Printf("%s verdict=%s states=%d (full %d) gen=%s\n",
		filepath.Base(path), c.Verdict, len(c.States), len(full.States), gen.Round(time.Microsecond))
}

func cmdVerify(args []string) {
	pos, flags := splitFlags(args)
	goFile := flags["go"]
	if len(pos) != 1 || goFile == "" {
		usage()
		os.Exit(2)
	}
	c, err := cert.Load(pos[0])
	if err != nil {
		fatal(err)
	}
	t0 := time.Now()
	fails := cert.Verify(c, goFile)
	d := time.Since(t0)
	if len(fails) == 0 {
		fmt.Printf("OK %s verdict=%s states=%d replay=%s\n",
			filepath.Base(goFile), c.Verdict, len(c.States), d.Round(time.Microsecond))
		return
	}
	for _, f := range fails {
		fmt.Println(f)
	}
	os.Exit(1)
}

func cmdRecheck(args []string) {
	fs := flag.NewFlagSet("recheck", flag.ExitOnError)
	fs.Parse(args)
	if fs.NArg() != 1 {
		usage()
		os.Exit(2)
	}
	path := fs.Arg(0)
	sha := sha256Of(path)
	p, err := model.Extract(path, sha)
	if err != nil {
		fatal(err)
	}
	if len(p.Outside) > 0 {
		fmt.Printf("OUTSIDE_BOUND %v\n", p.Outside)
		return
	}
	fmt.Printf("  chan_cap=%v tasks=%d spawned=%d locs=%v\n", p.ChanCap, len(p.Tasks), p.NTasks, p.Locs)
	m := lts.NewMachine(p)
	full, err := m.Enumerate(false, 2000000)
	if err != nil {
		fatal(err)
	}
	por, err := m.Enumerate(true, 100000)
	if err != nil {
		fatal(err)
	}
	v := "CERTIFIED"
	if full.Race {
		v = "RACE_DETECTED"
	}
	fmt.Printf("%s verdict=%s full_states=%d por_states=%d por_used=%v pruned=%d por_agrees=%v\n",
		filepath.Base(path), v, len(full.States), len(por.States), por.PORUsed, len(por.Pruned), full.Race == por.Race)
}

// ---- corpus mode ----

type Row struct {
	Program    string   `json:"program"`
	GoFile     string   `json:"go_file"`
	Expected   string   `json:"expected"`
	ExpectedID string   `json:"expected_rule"`
	Verdict    string   `json:"verdict"`
	Status     string   `json:"status"` // CERTIFIED | RACE_DETECTED | OUTSIDE_BOUND
	Outside    []string `json:"outside,omitempty"`
	NTasks     int      `json:"n_tasks"`
	SumChanCap int      `json:"sum_chan_cap"`
	Tier       string   `json:"tier"`
	States     int      `json:"states"`
	FullStates int      `json:"full_states"`
	PORUsed    bool     `json:"por_used"`
	PORAgrees  bool     `json:"por_agrees"`
	GenUS      int64    `json:"gen_us"`
	VerifyUS   int64    `json:"verify_us"`
	VerifyOK   bool     `json:"verify_ok"`
	Failures   []string `json:"failures,omitempty"`
	RacePath   []string `json:"race_path,omitempty"`
}

var verdictRe = regexp.MustCompile(`(?m)^//\s*EXPECTED-VERDICT:\s*(ACCEPT-MANAGED|ACCEPT|REJECT)(?:\s+([A-Za-z0-9_\-]+))?`)

func expectedOf(path string) (string, string) {
	b, err := os.ReadFile(path)
	if err != nil {
		return "", ""
	}
	m := verdictRe.FindStringSubmatch(string(b))
	if m == nil {
		return "", ""
	}
	return m[1], m[2]
}

func cmdCorpus(args []string) {
	fs := flag.NewFlagSet("corpus", flag.ExitOnError)
	corpus := fs.String("corpus", "../../tests/checker-corpus", "corpus directory")
	goDir := fs.String("go", "", "directory of emitted Go files (<stem>.go)")
	out := fs.String("out", "results.json", "results json")
	certDir := fs.String("certs", "", "optional certificate output directory")
	extended := fs.Bool("extended", false, "raise the channel-depth budget to 16 (reported separately)")
	fs.Parse(args)
	if *goDir == "" {
		usage()
		os.Exit(2)
	}
	if *extended {
		model.ChanBound = 16
	}
	entries, err := os.ReadDir(*corpus)
	if err != nil {
		fatal(err)
	}
	var rows []Row
	for _, e := range entries {
		if !strings.HasSuffix(e.Name(), ".hy") {
			continue
		}
		stem := strings.TrimSuffix(e.Name(), ".hy")
		exp, rule := expectedOf(filepath.Join(*corpus, e.Name()))
		row := Row{Program: stem, Expected: exp, ExpectedID: rule}
		goFile := filepath.Join(*goDir, stem+".go")
		if _, err := os.Stat(goFile); err != nil {
			row.Status = "NOT_EMITTED"
			rows = append(rows, row)
			continue
		}
		row.GoFile = goFile
		sha := sha256Of(goFile)
		p, err := model.Extract(goFile, sha)
		if err != nil {
			row.Status = "EXTRACT_ERROR"
			row.Outside = []string{err.Error()}
			rows = append(rows, row)
			continue
		}
		row.NTasks = p.NTasks
		row.SumChanCap = p.SumChanCap
		row.Tier = p.Tier
		if len(p.Outside) > 0 {
			row.Status = "OUTSIDE_BOUND"
			row.Outside = p.Outside
			rows = append(rows, row)
			continue
		}
		m := lts.NewMachine(p)
		t0 := time.Now()
		full, err := m.Enumerate(false, 2000000)
		if err != nil {
			row.Status = "OUTSIDE_BOUND"
			row.Outside = []string{err.Error()}
			rows = append(rows, row)
			continue
		}
		por, err := m.Enumerate(true, 100000)
		if err != nil {
			row.Status = "OUTSIDE_BOUND"
			row.Outside = []string{err.Error()}
			rows = append(rows, row)
			continue
		}
		row.GenUS = time.Since(t0).Microseconds()
		c := cert.Emit(p, m, full, por)
		row.States = len(c.States)
		row.FullStates = len(full.States)
		row.PORUsed = por.PORUsed
		row.PORAgrees = full.Race == por.Race
		if por.Race {
			row.Status = "RACE_DETECTED"
			for _, e := range por.RacePath {
				row.RacePath = append(row.RacePath, e.Label)
			}
		} else {
			row.Status = "CERTIFIED"
		}
		if *certDir != "" {
			os.MkdirAll(*certDir, 0o755)
			c.Save(filepath.Join(*certDir, stem+".hcert"))
		}
		t1 := time.Now()
		fails := cert.Verify(c, goFile)
		row.VerifyUS = time.Since(t1).Microseconds()
		row.VerifyOK = len(fails) == 0
		row.Failures = fails
		rows = append(rows, row)
	}
	sort.Slice(rows, func(i, j int) bool { return rows[i].Program < rows[j].Program })
	b, _ := json.MarshalIndent(rows, "", " ")
	os.WriteFile(*out, append(b, '\n'), 0o644)

	// console summary
	var cert, race, outside, notEmitted, bad int
	for _, r := range rows {
		switch r.Status {
		case "CERTIFIED":
			cert++
		case "RACE_DETECTED":
			race++
		case "OUTSIDE_BOUND":
			outside++
		case "NOT_EMITTED":
			notEmitted++
		}
		if !r.VerifyOK && r.Status != "NOT_EMITTED" && r.Status != "OUTSIDE_BOUND" {
			bad++
		}
		if r.Status != "NOT_EMITTED" && r.Status != "OUTSIDE_BOUND" && !r.PORAgrees {
			bad++
		}
	}
	fmt.Printf("corpus=%d certified=%d race=%d outside=%d not_emitted=%d verify_failures=%d\n",
		len(rows), cert, race, outside, notEmitted, bad)
}

// splitFlags accepts "-name value" and "--name value" anywhere in args.
func splitFlags(args []string) ([]string, map[string]string) {
	var pos []string
	flags := map[string]string{}
	for i := 0; i < len(args); i++ {
		a := args[i]
		if strings.HasPrefix(a, "-") {
			name := strings.TrimLeft(a, "-")
			if i+1 < len(args) {
				flags[name] = args[i+1]
				i++
			}
			continue
		}
		pos = append(pos, a)
	}
	return pos, flags
}

func fatal(err error) {
	fmt.Fprintln(os.Stderr, "error:", err)
	os.Exit(1)
}
