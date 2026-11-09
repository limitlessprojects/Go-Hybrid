// 09-go-imports-hybrid — THE ADOPTION STORY (the Hybrid side of it). This
// package compiles to plain Go (package hybrid) that a Go program imports
// with NO Hybrid toolchain: the generated source + the prebuilt runtime
// pieces ship with it (spike 2's zero-install route, landed by `hyb build
// --package`). validate_email calls THROUGH the pinned rust/regex crate.

import "rust/regex"

// validate_email: the pinned email-shape regex, evaluated by the Rust
// regex crate through the boundary. Error is a Hybrid error value (the
// Go consumer sees it as a plain error).
pub fn validate_email(s string) (string, error) {
    ok := regex.is_match(`^[^@\s]+@[^@\s]+\.[^@\s]+$`, s)?
    if !ok {
        return _, errors.new("invalid email address")
    }
    return s, nil
}

// simple_pattern: does the pattern compile as a valid regex? (validity as
// a bool — no error channel; the consumer's smoke call pins `match: true`.)
pub fn simple_pattern(p string) bool {
    return regex.is_valid(p)
}
