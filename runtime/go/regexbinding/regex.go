package regexbinding

import (
	"errors"

	"hybrid.program/hybridge"
)

var ErrRegex = errors.New("regex: pattern or text invalid")

func IsMatch(pattern, text string) (bool, error) {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return false, ErrRegex
	}

	arena := hybridge.GetOwnHandle(len(pattern) + len(text) + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	var pptr uint32
	if len(pattern) > 0 {
		var pslice []byte
		pptr, pslice, err = arena.Alloc(len(pattern))
		if err != nil {
			return false, ErrRegex
		}
		copy(pslice, pattern)
		bridge.Memory().Write(pptr, pslice)
	}

	var tptr uint32
	if len(text) > 0 {
		var tslice []byte
		tptr, tslice, err = arena.Alloc(len(text))
		if err != nil {
			return false, ErrRegex
		}
		copy(tslice, text)
		bridge.Memory().Write(tptr, tslice)
	}

	res := bridge.HyRegexIsMatch(pptr, uint32(len(pattern)), tptr, uint32(len(text)))
	if res < 0 {
		return false, ErrRegex
	}
	return res == 1, nil
}

func IsValid(pattern string) bool {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return false
	}

	arena := hybridge.GetOwnHandle(len(pattern) + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	var pptr uint32
	if len(pattern) > 0 {
		var pslice []byte
		pptr, pslice, err = arena.Alloc(len(pattern))
		if err != nil {
			return false
		}
		copy(pslice, pattern)
		bridge.Memory().Write(pptr, pslice)
	}

	res := bridge.HyRegexIsValid(pptr, uint32(len(pattern)))
	return res == 1
}
