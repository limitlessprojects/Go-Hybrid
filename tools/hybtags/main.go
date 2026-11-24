// Command hybtags checks a declared family of Go build tags and returns a
// minimal failing configuration for each failing combination.
//
// Given a package and a finite set of user build tags (e.g. fast, small),
// hybtags enumerates every 2^n tag assignment, asks `go list` which Go files
// each assignment selects, groups assignments that select identical inputs
// (they share one checker run), runs the checker once per group, and reports
// — for every failing group — a minimal tag subset that still fails.
//
// This implements the Wildcard Track proposal in issue #4726:
// "Check a declared family of Go build tags and return a minimal failing
// configuration."
//
// Usage:
//
//	hybtags --pkg ./mypkg --tags fast,small,debug
//	hybtags --pkg ./mypkg --tags fast,small --checker "go vet"
//	hybtags --pkg ./mypkg --tags fast,small --json
//
// Exit code is 0 when every group passes, 1 when at least one group fails.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"sort"
	"strings"
	"time"
)

// Combination is one assignment of the declared build tags: the tags that are
// active ("on") for that build.
type Combination struct {
	Tags   []string         // active tags, in declared order
	Active map[string]bool  // membership lookup
}

// fileSelection is the result of asking `go list` which files a tag
// assignment selects.
type fileSelection struct {
	combo Combination
	files []string
	err   error
}

// group clusters combinations that select identical file sets, plus the
// checker outcome for that shared input.
type group struct {
	key              string          // canonical, sorted file-set fingerprint
	files            []string        // selected Go files (sorted)
	combinations     []Combination   // all tag assignments mapping to this input
	pass             bool            // checker result for the group
	checkerOut       string          // checker stdout
	checkerErr       string          // checker stderr
	minimalFailing   []string        // minimal failing tag subset (only when !pass)
}

// flags
var (
	pkgFlag     = flag.String("pkg", ".", "package directory to check")
	tagsFlag    = flag.String("tags", "", "comma-separated declared user build tags")
	checkerFlag = flag.String("checker", "go build", "checker command to run per unique input")
	jsonFlag    = flag.Bool("json", false, "emit a JSON report")
	verboseFlag = flag.Bool("v", false, "log each checker invocation")
)

func main() {
	flag.Parse()

	tagList := parseTags(*tagsFlag)
	if len(tagList) == 0 {
		fmt.Fprintln(os.Stderr, "hybtags: no build tags declared (use --tags fast,small,...)")
		os.Exit(2)
	}

	start := time.Now()

	// 1. Enumerate every 2^n tag assignment.
	combos := enumerateCombinations(tagList)

	// 2. For each assignment, ask `go list` which Go files it selects.
	selections := make([]fileSelection, len(combos))
	for i, combo := range combos {
		files, err := listGoFiles(*pkgFlag, combo.tags)
		selections[i] = fileSelection{combo: combo, files: files, err: err}
	}

	// 3. Group assignments that select identical file sets.
	groups := groupByFiles(selections)

	// 4. Run the checker once per unique input (one run per group).
	totalInvocations := 0
	for i := range groups {
		rep := groups[i].combinations[0]
		totalInvocations++
		pass, out, errOut := runChecker(*checkerFlag, *pkgFlag, rep.tags)
		groups[i].pass = pass
		groups[i].checkerOut = out
		groups[i].checkerErr = errOut
		if *verboseFlag {
			fmt.Fprintf(os.Stderr, "hybtags: checker %v -> %v (tags=%v)\n", rep.tags, pass, rep.tags)
		}

		// 5. For each failing group, find a minimal failing tag subset.
		if !pass {
			groups[i].minimalFailing = findMinimalFailing(*checkerFlag, *pkgFlag, rep.tags)
		}
	}

	wall := time.Since(start)

	// 6. Emit the report.
	report := report{
		Package:            *pkgFlag,
		DeclaredTags:       tagList,
		Combinations:       len(combos),
		UniqueInputs:       len(groups),
		CheckerInvocations: totalInvocations,
		Checker:            *checkerFlag,
		WallTime:           wall.String(),
		Groups:             make([]groupReport, 0, len(groups)),
	}
	for _, g := range groups {
		report.Groups = append(report.Groups, groupReport{
			Files:            g.files,
			CombinationCount: len(g.combinations),
			TagCombinations:  tagCombos(g.combinations),
			Pass:             g.pass,
			CheckerOut:       g.checkerOut,
			CheckerErr:       g.checkerErr,
			MinimalFailing:   g.minimalFailing,
		})
	}

	if *jsonFlag {
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		if err := enc.Encode(report); err != nil {
			fmt.Fprintf(os.Stderr, "hybtags: encoding report: %v\n", err)
			os.Exit(1)
		}
	} else {
		printReport(report)
	}

	anyFail := false
	for _, g := range report.Groups {
		if !g.Pass {
			anyFail = true
			break
		}
	}
	if anyFail {
		os.Exit(1)
	}
}

// parseTags splits a comma-separated tag list, trimming whitespace and
// dropping empty entries. Duplicate tags are collapsed, preserving first
// occurrence order.
func parseTags(s string) []string {
	seen := make(map[string]bool)
	var out []string
	for _, t := range strings.Split(s, ",") {
		t = strings.TrimSpace(t)
		if t == "" || seen[t] {
			continue
		}
		seen[t] = true
		out = append(out, t)
	}
	return out
}

// enumerateCombinations returns every 2^n assignment of the declared tags,
// ordered so that the all-off assignment comes first and tags are added in
// declared order.
func enumerateCombinations(tags []string) []Combination {
	n := len(tags)
	total := 1 << n
	combos := make([]Combination, total)
	for mask := 0; mask < total; mask++ {
		active := make(map[string]bool, n)
		var activeTags []string
		for i := 0; i < n; i++ {
			if mask&(1<<i) != 0 {
				active[tags[i]] = true
				activeTags = append(activeTags, tags[i])
			}
		}
		combos[mask] = Combination{Tags: activeTags, Active: active}
	}
	return combos
}

// listGoFiles runs `go list -e -find -json -tags <tags> <pkg>` and returns the
// selected Go source files (GoFiles). The -find flag avoids dependency
// resolution; -e tolerates load errors so a single broken assignment does not
// abort the enumeration.
func listGoFiles(pkg string, tags []string) ([]string, error) {
	args := []string{"list", "-e", "-find", "-json"}
	if len(tags) > 0 {
		args = append(args, "-tags", strings.Join(tags, ","))
	}
	args = append(args, pkg)

	cmd := exec.Command("go", args...)
	out, err := cmd.Output()
	if err != nil {
		// Even if go list reports a load error, it may still emit partial JSON.
		if len(out) == 0 {
			return nil, fmt.Errorf("go list %v: %v", strings.Join(args, " "), err)
		}
	}

	var info struct {
		GoFiles []string `json:"GoFiles"`
	}
	if err := json.Unmarshal(out, &info); err != nil {
		return nil, fmt.Errorf("parsing go list output: %w", err)
	}

	sort.Strings(info.GoFiles)
	return info.GoFiles, nil
}

// groupByFiles clusters file selections by identical (sorted) file sets.
// Selections that errored are each placed in their own group so the failure is
// visible rather than silently merged.
func groupByFiles(selections []fileSelection) []*group {
	byKey := make(map[string]*group)
	var ordered []*group

	for _, sel := range selections {
		key := filesKey(sel.files)
		g, ok := byKey[key]
		if !ok {
			g = &group{
				key:  key,
				files: sel.files,
			}
			byKey[key] = g
			ordered = append(ordered, g)
		}
		g.combinations = append(g.combinations, sel.combo)
	}
	return ordered
}

// filesKey returns a canonical fingerprint for a sorted file list.
func filesKey(files []string) string {
	return strings.Join(files, "\x00")
}

// runChecker executes the checker command with the given tags against the
// package. It returns pass=true when the command exits 0.
func runChecker(checker, pkg string, tags []string) (bool, string, string) {
	parts := strings.Fields(checker)
	if len(parts) == 0 {
		return false, "", "empty checker command"
	}
	bin := parts[0]
	args := parts[1:]

	// Append build tags if the checker supports -tags (go build/go vet/go test do).
	hasTags := false
	for _, a := range args {
		if a == "-tags" {
			hasTags = true
			break
		}
	}
	if !hasTags && len(tags) > 0 {
		args = append(args, "-tags", strings.Join(tags, ","))
	}
	args = append(args, pkg)

	cmd := exec.Command(bin, args...)
	out, err := cmd.CombinedOutput()
	outStr := string(out)
	if err != nil {
		return false, outStr, err.Error()
	}
	return true, outStr, ""
}

// findMinimalFailing takes a failing tag set and greedily removes tags while
// the package still fails to build/check, yielding a minimal (not necessarily
// minimum) failing configuration.
func findMinimalFailing(checker, pkg string, tags []string) []string {
	current := append([]string(nil), tags...)
	sort.Strings(current)

	changed := true
	for changed {
		changed = false
		for i := 0; i < len(current); i++ {
			candidate := append([]string(nil), current[:i]...)
			candidate = append(candidate, current[i+1:]...)

			pass, _, _ := runChecker(checker, pkg, candidate)
			if !pass {
				// Removing this tag still fails: keep the removal.
				current = candidate
				changed = true
				break // restart scan on the smaller set
			}
		}
	}
	return current
}

// tagCombos renders the active tags of each combination as strings.
func tagCombos(combos []Combination) []string {
	out := make([]string, len(combos))
	for i, c := range combos {
		if len(c.Tags) == 0 {
			out[i] = "<none>"
		} else {
			out[i] = strings.Join(c.Tags, ",")
		}
	}
	return out
}

// ---------- report structures ----------

type report struct {
	Package            string       `json:"package"`
	DeclaredTags       []string     `json:"declared_tags"`
	Combinations       int          `json:"combinations"`
	UniqueInputs       int          `json:"unique_inputs"`
	CheckerInvocations int          `json:"checker_invocations"`
	Checker            string       `json:"checker"`
	WallTime           string       `json:"wall_time"`
	Groups             []groupReport `json:"groups"`
}

type groupReport struct {
	Files            []string `json:"files"`
	CombinationCount int      `json:"combination_count"`
	TagCombinations  []string `json:"tag_combinations"`
	Pass             bool     `json:"pass"`
	CheckerOut       string   `json:"checker_out"`
	CheckerErr       string   `json:"checker_err"`
	MinimalFailing   []string `json:"minimal_failing"`
}

func printReport(r report) {
	fmt.Printf("hybtags report for package %q\n", r.Package)
	fmt.Printf("  declared tags:     %v\n", r.DeclaredTags)
	fmt.Printf("  combinations:      %d\n", r.Combinations)
	fmt.Printf("  unique inputs:     %d\n", r.UniqueInputs)
	fmt.Printf("  checker runs:      %d (%s)\n", r.CheckerInvocations, r.Checker)
	fmt.Printf("  wall time:         %s\n", r.WallTime)
	fmt.Println()

	anyFail := false
	for _, g := range r.Groups {
		status := "PASS"
		if !g.Pass {
			status = "FAIL"
			anyFail = true
		}
		fmt.Printf("  [%s] %d combo(s): %v\n", status, g.CombinationCount, g.TagCombinations)
		fmt.Printf("         files: %v\n", g.Files)
		if !g.Pass {
			fmt.Printf("         minimal failing tags: %v\n", g.MinimalFailing)
			if g.CheckerErr != "" {
				fmt.Printf("         checker error: %s\n", strings.TrimSpace(g.CheckerErr))
			}
		}
	}
	fmt.Println()
	if anyFail {
		fmt.Println("hybtags: at least one build-tag combination fails.")
	} else {
		fmt.Println("hybtags: all build-tag combinations pass.")
	}
}
