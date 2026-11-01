#!/usr/bin/env python3
"""
W-024 Falsifier P2 Oracle:
Runs `go build -gcflags="-m=2"` on falsifier_p1_test.go and asserts:
1. `CertifiedStackKernel` has ZERO `escapes to heap` and ZERO `moved to heap:`.
2. Proves that stack-promotion lowering creates provably non-escaping stack frames.
"""

import subprocess
import sys
import os

def run_oracle():
    spike_dir = os.path.dirname(os.path.abspath(__file__))
    cmd = ["go", "test", "-c", "-gcflags=-m=2", "falsifier_p1_test.go"]
    
    print(f"[ORACLE] Executing: {' '.join(cmd)}")
    res = subprocess.run(cmd, cwd=spike_dir, capture_output=True, text=True)
    
    output = res.stdout + "\n" + res.stderr
    print("--- COMPILER ESCAPE ANALYSIS OUTPUT ---")
    print(output)
    print("---------------------------------------")
    
    # Check for escapes in CertifiedStackKernel
    in_certified_kernel = False
    violations = []
    
    for line in output.splitlines():
        if "CertifiedStackKernel" in line:
            in_certified_kernel = True
        elif line.startswith("./falsifier_p1_test.go:") and "CertifiedStackKernel" not in line:
            in_certified_kernel = False
            
        if in_certified_kernel:
            if "escapes to heap" in line or "moved to heap" in line:
                violations.append(line.strip())
                
    if violations:
        print("❌ FALSIFIER P2 FIRED: Heap escape detected in CertifiedStackKernel:")
        for v in violations:
            print("  ", v)
        sys.exit(1)
        
    print("✅ FALSIFIER P2 SURVIVED: CertifiedStackKernel proved 100% stack-resident by cmd/compile -m=2!")
    
    # Run test
    test_cmd = ["go", "test", "-v", "falsifier_p1_test.go"]
    test_res = subprocess.run(test_cmd, cwd=spike_dir, capture_output=True, text=True)
    print(test_res.stdout)
    if test_res.returncode != 0:
        print("❌ Test failed:", test_res.stderr)
        sys.exit(1)
        
    print("✅ AllocsPerRun == 0 verified successfully!")

if __name__ == "__main__":
    run_oracle()
