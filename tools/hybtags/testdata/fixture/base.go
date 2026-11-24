// Package fixture is a test fixture for hybtags. Different source files are
// selected depending on which user build tags are active, so that distinct tag
// assignments can (and do) map to identical `go list` file selections.
package fixture

// Base is always compiled, regardless of build tags.
func Base() string { return "base" }
