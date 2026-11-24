//go:build fast || debug

package fixture

// FastOrDebug is compiled when either `fast` or `debug` is active. This makes
// the `{fast}` and `{fast,debug}` assignments select the same files and thus
// share one checker run.
package fixture

func FastOrDebug() string { return "fast-or-debug" }
