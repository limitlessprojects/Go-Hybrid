// ============================================================================
// typed-const-expression-controls-accept — Checker corpus (issue #725).
//
// PURPOSE: the ACCEPT half of the typed-constant-expression rule. Every shape
// here is a legal neighbour that an over-broad version of the check would
// reject, and each was verified to build before this fixture was committed.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE); SPEC-LANGUAGE X1/X2
// (overflow WRAPS) and X4 (conversions are explicit and truncating).
//
// ---- Why this is an ACCEPT ----
//  1. SPEC X4's own example: `u8(300)` is 44, not an error. A conversion of a
//     constant is a defined truncation, so it must keep building.
//  2. SPEC X1/X2: a RUN-TIME value wraps. `var x u8 = 200; var y u8 = 100;
//     x + y` is not a constant expression — Go compiles it and the wrap is
//     defined behaviour. This is the single most important control in the
//     pair: a check that folds operands without proving they are constant
//     would reject this and break working programs.
//  3. In-range typed constants (`u8(100) + u8(20)`, `i64(1) << 62`) and a
//     `const` declaration whose value fits (`const top u8 = 255`) stay legal.
//  4. A comparison yields a bool, so `u8(255) > 1` is not a value of type
//     `u8` and must not be range-checked against it.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

const top u8 = 255
const mid u8 = 100

fn main() {
    // 1. SPEC X4 truncating conversion.
    t := u8(300)
    fmt.println("{t}")

    // 2. Run-time arithmetic on narrow types: WRAPS, must build.
    var x u8 = 200
    var y u8 = 100
    s := x + y
    fmt.println("{s}")
    var z u8 = 255
    w := z + 1
    fmt.println("{w}")

    // 3. In-range typed constants and a fitting const declaration.
    a := u8(100) + u8(20)
    b := i64(1) << 62
    c := u8(255)
    d := mid + 1
    fmt.println("{a} {b} {c} {d} {top}")

    // 4. A comparison is a bool, not a `u8` value.
    e := u8(255) > 1
    fmt.println("{e}")

    // Untyped arithmetic takes its type from context.
    f := 1 + 2
    g := 1.5 + 2.5
    fmt.println("{f} {g}")
}
