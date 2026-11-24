//go:build small

package fixture

// Small is compiled only when the `small` tag is active.
func Small() string { return "small" }
