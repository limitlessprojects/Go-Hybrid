// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
// Pre-registered 2026-10-15 before running this fixture against the checker.
// Rationale: issue #134 form 2. The emitter carries every declared Go
// import into the emitted header, and the root `strings` is never
// mentioned — Go rejects the module with `"strings" imported and not
// used`. Rejected fail-closed as HYB-E1002 (rule BUILDABLE).
import "strings"

fn main() {
    fmt.println("halo")
}
