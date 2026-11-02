package hy

// hyLive represents a live, owned capability.
// The type parameter is used to distinguish different roots if needed,
// but for Tier-0 we use struct{}.
type hyLive[Root any] struct{}

// hyMoved represents a capability that has been moved.
type hyMoved struct{}

// hyFrozen represents a capability that is currently frozen (e.g. loaned).
type hyFrozen struct{}

// hyUse asserts that a capability is currently Live.
// If the shadowed hyCap_x is hyMoved or hyFrozen, cmd/compile will
// reject this call.
//
// Note: In the emitted Go code, hyUse is called as hyUse(hyCap_x).
// Since hyUse expects hyLive[struct{}], passing a hyMoved or hyFrozen
// value will result in a compilation error.
func hyUse(_ hyLive[struct{}]) {}
