// Command revsynth checks every case, emits Go forward/inverse pairs for the
// admitted ones, and prints the admission and code-size tables.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"text/tabwriter"

	"revreg/ir"
)

type record struct {
	File     string `json:"file"`
	Name     string `json:"name"`
	Expect   string `json:"expect"`
	Admitted bool   `json:"admitted"`
	Code     string `json:"code,omitempty"`
	Msg      string `json:"msg,omitempty"`
	Line     int    `json:"line,omitempty"`
	GoName   string `json:"goName,omitempty"`
	IfCount  int    `json:"ifCount"`
	FwdBytes int    `json:"fwdBytes,omitempty"`
	FwdLines int    `json:"fwdLines,omitempty"`
	InvBytes int    `json:"invBytes,omitempty"`
	InvLines int    `json:"invLines,omitempty"`
	Match    bool   `json:"predictionHeld"`
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "revsynth:", err)
		os.Exit(1)
	}
}

func loadDir(dir string) ([]record, map[string]*ir.Program, error) {
	files, err := filepath.Glob(filepath.Join(dir, "*.rev"))
	if err != nil {
		return nil, nil, err
	}
	if len(files) == 0 {
		return nil, nil, fmt.Errorf("no .rev files in %s", dir)
	}
	sort.Strings(files)
	var recs []record
	progs := map[string]*ir.Program{}
	for _, f := range files {
		p, err := ir.ParseFile(f)
		if err != nil {
			return nil, nil, err
		}
		diags := ir.Check(p)
		r := record{File: filepath.Base(f), Name: p.Name, Expect: p.MetaOr("expect", ""), IfCount: p.IfCount}
		if len(diags) == 0 {
			r.Admitted = true
			r.GoName = ir.GoName(p.Name)
			r.Match = r.Expect == "ADMIT"
		} else {
			r.Code = diags[0].Code
			r.Msg = diags[0].Msg
			r.Line = diags[0].Line
			r.Match = r.Expect == r.Code
		}
		progs[p.Name] = p
		recs = append(recs, r)
	}
	return recs, progs, nil
}

func run() error {
	pos, posProg, err := loadDir(filepath.Join("cases", "pos"))
	if err != nil {
		return err
	}
	neg, _, err := loadDir(filepath.Join("cases", "neg"))
	if err != nil {
		return err
	}
	corp, _, err := loadDir(filepath.Join("corpus"))
	if err != nil {
		return err
	}

	fmt.Println("== 20 个例子（10 正 / 10 负）：检查器判定 vs 预注册 ==")
	tw := tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)
	fmt.Fprintln(tw, "file\tverdict\tcode\texpect\tok\tif")
	for _, r := range append(append([]record{}, pos...), neg...) {
		v := "ADMIT"
		if !r.Admitted {
			v = "REJECT"
		}
		fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%v\t%d\n", r.File, v, r.Code, r.Expect, r.Match, r.IfCount)
	}
	tw.Flush()

	// Distinctness of negative diagnostics.
	codes := map[string]string{}
	for _, r := range neg {
		if r.Admitted {
			return fmt.Errorf("negative case %s was admitted", r.Name)
		}
		if prev, dup := codes[r.Code]; dup {
			return fmt.Errorf("negative cases %s and %s share diagnostic %s", prev, r.Name, r.Code)
		}
		codes[r.Code] = r.Name
	}
	fmt.Printf("\n负例诊断码互不相同：%d/10 unique\n", len(codes))

	// Corpus prediction table.
	fmt.Println("\n== 24 条常见变换语料：预注册判定 vs 实际 ==")
	tw = tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)
	fmt.Fprintln(tw, "file\tverdict\tcode\texpect\tok")
	admitted := 0
	byCode := map[string]int{}
	for _, r := range corp {
		v := "ADMIT"
		if r.Admitted {
			admitted++
		} else {
			v = "REJECT"
			byCode[r.Code]++
		}
		fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%v\n", r.File, v, r.Code, r.Expect, r.Match)
	}
	tw.Flush()
	fmt.Printf("\n语料接纳率：%d/%d = %.1f%%\n", admitted, len(corp), 100*float64(admitted)/float64(len(corp)))

	// Emit.
	var emit []ir.EmitCase
	for _, r := range pos {
		if !r.Admitted {
			return fmt.Errorf("positive case %s was refused: %s: %s", r.Name, r.Code, r.Msg)
		}
		emit = append(emit, ir.EmitCase{Prog: posProg[r.Name], GoName: r.GoName, Kind: "pos"})
	}
	src, sizes, err := ir.EmitFile(emit)
	if err != nil {
		return err
	}
	if err := os.MkdirAll("gen", 0o755); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join("gen", "gen_cases.go"), []byte(src), 0o644); err != nil {
		return err
	}

	fmt.Println("\n== 发射的 Go 代码规模（字节 / 行）==")
	tw = tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)
	fmt.Fprintln(tw, "case\tfwd_bytes\tfwd_lines\tinv_bytes\tinv_lines\tratio")
	totF, totI := 0, 0
	for i := range pos {
		s := sizes[pos[i].Name]
		pos[i].FwdBytes, pos[i].FwdLines = s.ForwardBytes, s.ForwardLines
		pos[i].InvBytes, pos[i].InvLines = s.InverseBytes, s.InverseLines
		ratio := float64(s.InverseBytes) / float64(s.ForwardBytes)
		totF += s.ForwardBytes
		totI += s.InverseBytes
		fmt.Fprintf(tw, "%s\t%d\t%d\t%d\t%d\t%.2f\n", pos[i].Name, s.ForwardBytes, s.ForwardLines, s.InverseBytes, s.InverseLines, ratio)
	}
	tw.Flush()
	fmt.Printf("合计 forward=%d B  inverse=%d B  ratio=%.2f\n", totF, totI, float64(totI)/float64(totF))

	// Machine-readable artefacts.
	if err := os.MkdirAll("out", 0o755); err != nil {
		return err
	}
	summary := map[string]any{
		"positive": pos, "negative": neg, "corpus": corp,
		"corpusAdmitted": admitted, "corpusTotal": len(corp),
		"corpusRejectedByCode": byCode,
		"emittedForwardBytes": totF, "emittedInverseBytes": totI,
	}
	b, err := json.MarshalIndent(summary, "", "  ")
	if err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join("out", "summary.json"), b, 0o644); err != nil {
		return err
	}

	// Prediction audit.
	bad := 0
	for _, r := range append(append(append([]record{}, pos...), neg...), corp...) {
		if !r.Match {
			bad++
			fmt.Printf("PREDICTION MISS: %s expected %q got %q/%q\n", r.Name, r.Expect, r.Code, r.Msg)
		}
	}
	if bad != 0 {
		return fmt.Errorf("%d prediction(s) did not hold", bad)
	}
	fmt.Printf("\n预测审计：%d/%d 条预注册判定成立；写出 gen/gen_cases.go 与 out/summary.json\n",
		len(pos)+len(neg)+len(corp), len(pos)+len(neg)+len(corp))
	return nil
}
