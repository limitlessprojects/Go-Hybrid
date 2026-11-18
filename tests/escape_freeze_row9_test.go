package escapefreeze_test

import (
    "testing"
    "github.com/limitlessprojects/Go-Hybrid/hy-check"
)

// Helper that runs the Hybrid checker on a source string and returns the
// diagnostics IDs that were emitted.
func runChecker(src string) []string {
    diags := hy_check.CheckSource(src)
    ids := make([]string, len(diags))
    for i, d := range diags {
        ids[i] = d.ID
    }
    return ids
}

// REJECT cases (should emit HYB‑E3014 or HYB‑E3013)
func TestReject_R1_PostEscapeMapWrite(t *testing.T) {
    src := `
package main
import "net/http"
func main() {
    m := map[string]int{"/": 1}
    http.HandleFunc("/", func() { _ = m["/"] })
    m["/"] = 2 // <- should be rejected
}
`
    ids := runChecker(src)
    expect(t, ids, "HYB-E3014")
}

func TestReject_R2_AliasPeerWrite(t *testing.T) {
    src := `
package main
import "net/http"
func main() {
    m := map[string]int{"/": 1}
    alias := m
    http.HandleFunc("/", func() { _ = alias["/"] })
    m["/"] = 2 // rejected via alias peer
}
`
    ids := runChecker(src)
    expect(t, ids, "HYB-E3014")
}

func TestReject_R3_LoopBackedgeWrite(t *testing.T) {
    src := `
package main
import "net/http"
func main() {
    m := map[string]int{"/": 1}
    for i := 0; i < 2; i++ {
        http.HandleFunc("/", func() { _ = m["/"] })
        m["/"] = i // write after escape inside loop backedge
    }
}
`
    ids := runChecker(src)
    expect(t, ids, "HYB-E3014")
}

func TestReject_R4_CapturedWritePreserved(t *testing.T) {
    src := `
package main
import "net/http"
func main() {
    var x int
    http.HandleFunc("/", func() { x = 1 }) // should still be HYB‑E3013
}
`
    ids := runChecker(src)
    expect(t, ids, "HYB-E3013")
}

func TestReject_R5_StructFieldMapProjection(t *testing.T) {
    src := `
package main
import "net/http"
type Config struct { Routes map[string]int }
func main() {
    cfg := Config{Routes: map[string]int{"/":1}}
    http.HandleFunc("/", func() { _ = cfg.Routes["/"] })
    cfg.Routes["/"] = 2 // rejected via projection
}
`
    ids := runChecker(src)
    expect(t, ids, "HYB-E3014")
}

func TestReject_R6_TransitiveAliasChain(t *testing.T) {
    src := `
package main
import "net/http"
func main() {
    m0 := map[string]int{"/":1}
    m1 := m0
    m2 := m1
    http.HandleFunc("/", func() { _ = m2["/"] })
    m0["/"] = 2 // rejected through transitive alias chain
}
`
    ids := runChecker(src)
    expect(t, ids, "HYB-E3014")
}

// ACCEPT cases (should emit no HYB‑E3014/E3013)
func TestAccept_A1_SetupWritesOnly(t *testing.T) {
    src := `
package main
import "net/http"
func main() {
    routes := map[string]int{"/":1}
    http.HandleFunc("/", func() { _ = routes["/"] })
    // no post‑escape mutation
}
`
    ids := runChecker(src)
    expectNone(t, ids, []string{"HYB-E3014", "HYB-E3013"})
}

func TestAccept_A2_PrimitiveCopyCapture(t *testing.T) {
    src := `
package main
import "net/http"
func main() {
    count := 0
    http.HandleFunc("/", func() { _ = count }) // primitive copy, safe
}
`
    ids := runChecker(src)
    expectNone(t, ids, []string{"HYB-E3014", "HYB-E3013"})
}

func TestAccept_A3_ManagedInterop(t *testing.T) {
    src := `
package main
import "net/http"
func main() {
    cfg := map[string]int{"/":1}
    managed("http", "interop", func() { http.HandleFunc("/", func(){ _ = cfg["/"] }) })
    cfg["/"] = 2 // allowed because of managed block
}
`
    ids := runChecker(src)
    expectNone(t, ids, []string{"HYB-E3014", "HYB-E3013"})
}

func TestAccept_A4_LocalSynchronousClosure(t *testing.T) {
    src := `
package main
func main() {
    m := map[string]int{"/":1}
    f := func() { _ = m["/"] }
    f() // synchronous, no foreign escape
    m["/"] = 2 // safe
}
`
    ids := runChecker(src)
    expectNone(t, ids, []string{"HYB-E3014", "HYB-E3013"})
}

func TestAccept_A5_MultipleForeignReaders(t *testing.T) {
    src := `
package main
import "net/http"
func main() {
    routes := map[string]int{"/":1}
    http.HandleFunc("/a", func(){ _ = routes["/a"] })
    http.HandleFunc("/b", func(){ _ = routes["/b"] })
    // no mutation after either registration
}
`
    ids := runChecker(src)
    expectNone(t, ids, []string{"HYB-E3014", "HYB-E3013"})
}

func TestAccept_A6_DisjointMapMutation(t *testing.T) {
    src := `
package main
import "net/http"
func main() {
    routes := map[string]int{"/":1}
    other := map[string]int{"x":0}
    http.HandleFunc("/", func(){ _ = routes["/"] })
    other["x"] = 1 // mutates a different map, safe
}
`
    ids := runChecker(src)
    expectNone(t, ids, []string{"HYB-E3014", "HYB-E3013"})
}

// Helper assertions
func expect(t *testing.T, got []string, want string) {
    for _, id := range got {
        if id == want {
            return
        }
    }
    t.Fatalf("expected diagnostic %s, got %v", want, got)
}

func expectNone(t *testing.T, got []string, forbidden []string) {
    for _, f := range forbidden {
        for _, id := range got {
            if id == f {
                t.Fatalf("unexpected diagnostic %s", f)
            }
        }
    }
}
