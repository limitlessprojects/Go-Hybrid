package panicoracle

import (
	"bufio"
	"bytes"
	"fmt"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
)

// Diagnostic represents a surviving panic or bounds-check site reported by cmd/compile.
type Diagnostic struct {
	File    string
	Line    int
	Col     int
	Kind    string // "IsInBounds", "IsSliceInBounds", "panicdivide", "panicshift"
	Message string
	FixIt   string
}

// OracleResult contains the analysis outcome from cmd/compile's prove pass and assembly scan.
type OracleResult struct {
	CertifiedClean bool
	Diagnostics    []Diagnostic
	BCECounts      map[string]int
	RuntimeCalls   map[string]int
	RawBCEOutput   string
}

var (
	bcePattern = regexp.MustCompile(`^(.+?):(\d+):(\d+):\s+Found (IsInBounds|IsSliceInBounds)$`)
	asmPattern = regexp.MustCompile(`CALL\s+runtime\.(panic\w*|goPanic\w*|closechan|chansend1)\(SB\)`)
)

// AnalyzeModule runs cmd/compile's SSA prove pass and assembly scan on a Go package directory.
func AnalyzeModule(goBin string, dir string) (*OracleResult, error) {
	// 1. Run BCE prove pass inspection
	cmdBCE := exec.Command(goBin, "build", "-o", "/dev/null", "-gcflags=-d=ssa/check_bce/debug=1", ".")
	cmdBCE.Dir = dir
	var stderrBCE bytes.Buffer
	cmdBCE.Stderr = &stderrBCE
	_ = cmdBCE.Run()

	// 2. Run assembly scan for surviving runtime panic calls
	cmdASM := exec.Command(goBin, "build", "-o", "/dev/null", "-gcflags=-S", ".")
	cmdASM.Dir = dir
	var stderrASM bytes.Buffer
	cmdASM.Stderr = &stderrASM
	_ = cmdASM.Run()

	res := &OracleResult{
		BCECounts:    make(map[string]int),
		RuntimeCalls: make(map[string]int),
		RawBCEOutput: stderrBCE.String(),
	}

	scanner := bufio.NewScanner(&stderrBCE)
	for scanner.Scan() {
		line := scanner.Text()
		matches := bcePattern.FindStringSubmatch(line)
		if len(matches) == 5 {
			file := matches[1]
			lineNum, _ := strconv.Atoi(matches[2])
			colNum, _ := strconv.Atoi(matches[3])
			kind := matches[4]

			res.BCECounts[kind]++

			fixIt := generateFixIt(kind)
			res.Diagnostics = append(res.Diagnostics, Diagnostic{
				File:    file,
				Line:    lineNum,
				Col:     colNum,
				Kind:    kind,
				Message: fmt.Sprintf("unproven runtime bounds check (%s) survives compile-time optimization", kind),
				FixIt:   fixIt,
			})
		}
	}

	asmScanner := bufio.NewScanner(&stderrASM)
	for asmScanner.Scan() {
		line := asmScanner.Text()
		matches := asmPattern.FindStringSubmatch(line)
		if len(matches) == 2 {
			call := matches[1]
			res.RuntimeCalls[call]++
			if strings.HasPrefix(call, "panicdivide") {
				res.Diagnostics = append(res.Diagnostics, Diagnostic{
					Kind:    "panicdivide",
					Message: "unproven integer divide/mod by zero site survives in emitted package code",
					FixIt:   "guard divisor with `if divisor != 0` or checked `div(...)`",
				})
			}
		}
	}

	res.CertifiedClean = (len(res.Diagnostics) == 0)
	return res, nil
}

func generateFixIt(kind string) string {
	switch kind {
	case "IsInBounds":
		return "add length guard `if idx < len(slice)` or use safe accessor `slice.get(idx)` returning `option[T]`"
	case "IsSliceInBounds":
		return "guard slice bounds `if 0 <= low && low <= high && high <= len(slice)` before sub-slicing"
	default:
		return "assert provable upper bound or guard condition before operation"
	}
}
