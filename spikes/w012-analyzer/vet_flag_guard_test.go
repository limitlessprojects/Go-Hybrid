package analyzer

import (
	"context"
	"io"
	"strings"
	"testing"
)

func TestRunVetRejectsOwnedFlagOverrides(t *testing.T) {
	tests := []struct {
		name string
		args []string
		want string
	}{
		{"vettool short equals", []string{"-vettool=/tmp/other-analyzer", "./..."}, "owns the analyzer binary"},
		{"vettool long equals", []string{"--vettool=/tmp/other-analyzer", "./..."}, "owns the analyzer binary"},
		{"vettool short separated", []string{"-vettool", "/tmp/other-analyzer", "./..."}, "owns the analyzer binary"},
		{"vettool long separated", []string{"--vettool", "/tmp/other-analyzer", "./..."}, "owns the analyzer binary"},
		{"cache rebuild short false", []string{"-a=false", "./..."}, "uncached vet action"},
		{"cache rebuild long false", []string{"--a=false", "./..."}, "uncached vet action"},
		{"cache rebuild short zero", []string{"-a=0", "./..."}, "uncached vet action"},
		{"cache rebuild long zero", []string{"--a=0", "./..."}, "uncached vet action"},
		{"cache rebuild short alone", []string{"-a", "./..."}, "uncached vet action"},
		{"cache rebuild long alone", []string{"--a", "./..."}, "uncached vet action"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := RunVet(context.Background(), tt.args, io.Discard, io.Discard)
			if err == nil || !strings.Contains(err.Error(), tt.want) {
				t.Fatalf("RunVet(%q) = %v; want an immediate owned-flag rejection containing %q", tt.args, err, tt.want)
			}
		})
	}
}
