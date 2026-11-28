package witness

// Zero-sized phantom capability markers
type hyLive[R any] struct{}
type hyMoved struct{}
type hyBorrowed[R any] struct{}

// Inlineable zero-cost static assertion helpers
func hyUse[R any](hyLive[R]) {}

func hyRequireLive[R any](hyLive[R]) {}

func hyRequireOwnedForMove[R any](hyLive[R]) {}

type RootA struct{}
type RootB struct{}
