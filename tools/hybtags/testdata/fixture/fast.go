//go:build fast

package fixture

// Fast is compiled only when the `fast` tag is active.
func Fast() string { return "fast" }
