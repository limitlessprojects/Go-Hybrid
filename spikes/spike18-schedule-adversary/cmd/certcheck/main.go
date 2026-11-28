package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"

	periodic "hybrid-spike18-adversary"
)

func run() error {
	input := flag.String("in", "", "graph and witness JSON file (required)")
	output := flag.String("out", "", "optional generated Go file")
	pkg := flag.String("package", "kernel", "generated package name")
	flag.Parse()
	if *input == "" || flag.NArg() != 0 {
		return fmt.Errorf("usage: certcheck -in INPUT.json [-out kernel.go] [-package kernel]")
	}
	f, err := os.Open(*input)
	if err != nil {
		return err
	}
	defer f.Close()
	in, err := periodic.Decode(f)
	if err != nil {
		return err
	}
	report, err := periodic.Verify(in.Graph, in.Certificate)
	if err != nil {
		return err
	}
	if *output != "" {
		source, err := periodic.Emit(in.Graph, in.Certificate, *pkg)
		if err != nil {
			return err
		}
		if err := os.WriteFile(*output, source, 0644); err != nil {
			return err
		}
	}
	return json.NewEncoder(os.Stdout).Encode(report)
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
