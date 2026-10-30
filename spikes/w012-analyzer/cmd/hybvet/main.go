package main

import (
	"context"
	"fmt"
	"os"

	analyzer "github.com/limitlessprojects/go-hybrid/spikes/w012-analyzer"
	"golang.org/x/tools/go/analysis/singlechecker"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "vet" {
		if err := analyzer.RunVet(context.Background(), os.Args[2:], os.Stdout, os.Stderr); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		return
	}
	singlechecker.Main(analyzer.Analyzer)
}
