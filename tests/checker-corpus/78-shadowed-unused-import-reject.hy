// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
// Registered 2026-10-17 before this corpus run, after the import-shadowing
// defect and fix were reproduced in prior local experiments.
// LX4 retains Go lexical scope: this read uses the local strings binding,
// not the explicit package import. Case 47 pins the plain unused form.
import "strings"

fn main() {
    strings := 1
    fmt.println(strings)
}
