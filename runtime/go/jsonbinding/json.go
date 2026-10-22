package jsonbinding

import (
	"encoding/binary"
	"encoding/json"
	"errors"
	"runtime"

	"hybrid.program/hybridge"
)

var (
	ErrJSON = errors.New("serde_json: operation failed")
	ErrABI  = errors.New("serde_json: hyrt ABI handshake mismatch")
)

type Value struct{ h *handle }

type handle struct{ id uint64 }

func wrap(id uint64) Value {
	h := &handle{id: id}
	runtime.AddCleanup(h, func(id uint64) {
		if id != 0 {
			if bridge, err := hybridge.GetWasmBridge(); err == nil {
				_, _ = bridge.HyJsonRelease(id)
			}
		}
	}, id)
	return Value{h: h}
}

func (v Value) Release() {
	if v.h != nil && v.h.id != 0 {
		if bridge, err := hybridge.GetWasmBridge(); err == nil {
			_, _ = bridge.HyJsonRelease(v.h.id)
		}
		v.h.id = 0
	}
}

func Parse(data []byte) (Value, error) {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return Value{}, ErrJSON
	}

	arena := hybridge.GetOwnHandle(len(data) + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	var dataPtr uint32
	if len(data) > 0 {
		var inputSlice []byte
		dataPtr, inputSlice, err = arena.Alloc(len(data))
		if err != nil {
			return Value{}, ErrJSON
		}
		copy(inputSlice, data)
		bridge.Memory().Write(dataPtr, inputSlice)
	}

	outHandlePtr, _, err := arena.Alloc(8)
	if err != nil {
		return Value{}, ErrJSON
	}

	st, err := bridge.HyJsonParse(dataPtr, uint32(len(data)), outHandlePtr)
	if err != nil || st != statusOK {
		return Value{}, ErrJSON
	}

	outBytes, ok := bridge.Memory().Read(outHandlePtr, 8)
	if !ok {
		return Value{}, ErrJSON
	}

	id := binary.LittleEndian.Uint64(outBytes)
	return wrap(id), nil
}

func NewObject() Value {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return Value{}
	}

	arena := hybridge.GetOwnHandle(64)
	defer hybridge.ReleaseOwnHandle(arena)

	outPtr, _, err := arena.Alloc(8)
	if err != nil {
		return Value{}
	}

	st, err := bridge.HyJsonNewObject(outPtr)
	if err != nil || st != statusOK {
		return Value{}
	}

	outBytes, ok := bridge.Memory().Read(outPtr, 8)
	if !ok {
		return Value{}
	}

	id := binary.LittleEndian.Uint64(outBytes)
	return wrap(id)
}

func NewArray() Value {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return Value{}
	}

	arena := hybridge.GetOwnHandle(64)
	defer hybridge.ReleaseOwnHandle(arena)

	outPtr, _, err := arena.Alloc(8)
	if err != nil {
		return Value{}
	}

	st, err := bridge.HyJsonNewArray(outPtr)
	if err != nil || st != statusOK {
		return Value{}
	}

	outBytes, ok := bridge.Memory().Read(outPtr, 8)
	if !ok {
		return Value{}
	}

	id := binary.LittleEndian.Uint64(outBytes)
	return wrap(id)
}

func (v Value) Array(key string) ([]Value, error) {
	if v.h == nil || v.h.id == 0 {
		return nil, ErrJSON
	}
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return nil, ErrJSON
	}

	arena := hybridge.GetOwnHandle(len(key) + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	kptr, kslice, err := arena.Alloc(len(key))
	if err != nil {
		return nil, ErrJSON
	}
	copy(kslice, key)
	bridge.Memory().Write(kptr, kslice)

	outPtr, _, err := arena.Alloc(8)
	if err != nil {
		return nil, ErrJSON
	}

	st, err := bridge.HyJsonGetArray(v.h.id, kptr, uint32(len(key)), outPtr)
	if err != nil || st != statusOK {
		return nil, ErrJSON
	}

	arrBytes, ok := bridge.Memory().Read(outPtr, 8)
	if !ok {
		return nil, ErrJSON
	}
	arrID := binary.LittleEndian.Uint64(arrBytes)

	lenPtr, _, err := arena.Alloc(8)
	if err != nil {
		return nil, ErrJSON
	}

	st, err = bridge.HyJsonArrayLen(arrID, lenPtr)
	if err != nil || st != statusOK {
		return nil, ErrJSON
	}

	lenBytes, ok := bridge.Memory().Read(lenPtr, 8)
	if !ok {
		return nil, ErrJSON
	}

	n := binary.LittleEndian.Uint64(lenBytes)
	out := make([]Value, n)

	elemPtr, _, err := arena.Alloc(8)
	if err != nil {
		return nil, ErrJSON
	}

	for i := uint64(0); i < n; i++ {
		st, err = bridge.HyJsonArrayGet(arrID, i, elemPtr)
		if err != nil || st != statusOK {
			return nil, ErrJSON
		}
		elemBytes, ok := bridge.Memory().Read(elemPtr, 8)
		if !ok {
			return nil, ErrJSON
		}
		eid := binary.LittleEndian.Uint64(elemBytes)
		out[i] = wrap(eid)
	}

	_, _ = bridge.HyJsonRelease(arrID)
	return out, nil
}

func (v Value) Int(key string) (int64, error) {
	if v.h == nil || v.h.id == 0 {
		return 0, ErrJSON
	}
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return 0, ErrJSON
	}

	arena := hybridge.GetOwnHandle(len(key) + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	kptr, kslice, err := arena.Alloc(len(key))
	if err != nil {
		return 0, ErrJSON
	}
	copy(kslice, key)
	bridge.Memory().Write(kptr, kslice)

	outPtr, _, err := arena.Alloc(8)
	if err != nil {
		return 0, ErrJSON
	}

	st, err := bridge.HyJsonGetInt(v.h.id, kptr, uint32(len(key)), outPtr)
	if err != nil || st != statusOK {
		return 0, ErrJSON
	}

	outBytes, ok := bridge.Memory().Read(outPtr, 8)
	if !ok {
		return 0, ErrJSON
	}

	n := int64(binary.LittleEndian.Uint64(outBytes))
	return n, nil
}

func (v Value) String(key string) (string, error) {
	if v.h == nil || v.h.id == 0 {
		return "", ErrJSON
	}
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return "", ErrJSON
	}

	arena := hybridge.GetOwnHandle(len(key) + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	kptr, kslice, err := arena.Alloc(len(key))
	if err != nil {
		return "", ErrJSON
	}
	copy(kslice, key)
	bridge.Memory().Write(kptr, kslice)

	outPtr, _, err := arena.Alloc(8)
	if err != nil {
		return "", ErrJSON
	}

	st, err := bridge.HyJsonGetString(v.h.id, kptr, uint32(len(key)), outPtr)
	if err != nil || st != statusOK {
		return "", ErrJSON
	}

	outBytes, ok := bridge.Memory().Read(outPtr, 8)
	if !ok {
		return "", ErrJSON
	}

	sid := binary.LittleEndian.Uint64(outBytes)
	s := valueText(sid)
	_, _ = bridge.HyJsonRelease(sid)
	return s, nil
}

func (v Value) PushString(s string) error {
	if v.h == nil || v.h.id == 0 {
		return ErrJSON
	}
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return ErrJSON
	}

	arena := hybridge.GetOwnHandle(len(s) + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	sptr, sslice, err := arena.Alloc(len(s))
	if err != nil {
		return ErrJSON
	}
	copy(sslice, s)
	bridge.Memory().Write(sptr, sslice)

	st, err := bridge.HyJsonArrayPushString(v.h.id, sptr, uint32(len(s)))
	if err != nil || st != statusOK {
		return ErrJSON
	}
	return nil
}

func (v Value) Set(key string, val any) error {
	if hv, ok := val.(Value); ok {
		return v.setHandle(key, hv)
	}
	b, err := json.Marshal(val)
	if err != nil {
		return err
	}
	bv, err := Parse(b)
	if err != nil {
		return err
	}
	return v.setHandle(key, bv)
}

func (v Value) setHandle(key string, val Value) error {
	if v.h == nil || v.h.id == 0 || val.h == nil || val.h.id == 0 {
		return ErrJSON
	}
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return ErrJSON
	}

	arena := hybridge.GetOwnHandle(len(key) + 64)
	defer hybridge.ReleaseOwnHandle(arena)

	kptr, kslice, err := arena.Alloc(len(key))
	if err != nil {
		return ErrJSON
	}
	copy(kslice, key)
	bridge.Memory().Write(kptr, kslice)

	st, err := bridge.HyJsonSet(v.h.id, kptr, uint32(len(key)), val.h.id)
	if err != nil || st != statusOK {
		return ErrJSON
	}
	val.h.id = 0 // moved
	return nil
}

func ToStringPretty(v Value) (string, error) {
	if v.h == nil || v.h.id == 0 {
		return "", ErrJSON
	}
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return "", ErrJSON
	}

	arena := hybridge.GetOwnHandle(64)
	defer hybridge.ReleaseOwnHandle(arena)

	lenPtr, _, err := arena.Alloc(8)
	if err != nil {
		return "", ErrJSON
	}

	st, err := bridge.HyJsonToStringPretty(v.h.id, 0, 0, lenPtr)
	if st == statusTryAgain {
		lenBytes, ok := bridge.Memory().Read(lenPtr, 8)
		if !ok {
			return "", ErrJSON
		}
		need := binary.LittleEndian.Uint64(lenBytes)
		if need > 0 {
			outArena := hybridge.GetOwnHandle(int(need) + 64)
			defer hybridge.ReleaseOwnHandle(outArena)

			outPtr, _, err := outArena.Alloc(int(need))
			if err != nil {
				return "", ErrJSON
			}

			outLenPtr, _, err := outArena.Alloc(8)
			if err != nil {
				return "", ErrJSON
			}

			st, err = bridge.HyJsonToStringPretty(v.h.id, outPtr, uint32(need), outLenPtr)
			if err == nil && st == statusOK {
				outLenBytes, ok := bridge.Memory().Read(outLenPtr, 8)
				if !ok {
					return "", ErrJSON
				}
				resLen := binary.LittleEndian.Uint64(outLenBytes)
				readBytes, ok := bridge.Memory().Read(outPtr, uint32(resLen))
				if ok {
					return string(readBytes), nil
				}
			}
		}
	}
	return "", ErrJSON
}

func valueText(id uint64) string {
	bridge, err := hybridge.GetWasmBridge()
	if err != nil {
		return ""
	}

	arena := hybridge.GetOwnHandle(64)
	defer hybridge.ReleaseOwnHandle(arena)

	lenPtr, _, err := arena.Alloc(8)
	if err != nil {
		return ""
	}

	st, err := bridge.HyJsonValueText(id, 0, 0, lenPtr)
	if st == statusTryAgain {
		lenBytes, ok := bridge.Memory().Read(lenPtr, 8)
		if !ok {
			return ""
		}
		need := binary.LittleEndian.Uint64(lenBytes)
		if need > 0 {
			outArena := hybridge.GetOwnHandle(int(need) + 64)
			defer hybridge.ReleaseOwnHandle(outArena)

			outPtr, _, err := outArena.Alloc(int(need))
			if err != nil {
				return ""
			}

			outLenPtr, _, err := outArena.Alloc(8)
			if err != nil {
				return ""
			}

			st, err = bridge.HyJsonValueText(id, outPtr, uint32(need), outLenPtr)
			if err == nil && st == statusOK {
				readBytes, ok := bridge.Memory().Read(outPtr, uint32(need))
				if ok {
					var s string
					if err := json.Unmarshal(readBytes, &s); err == nil {
						return s
					}
					raw := string(readBytes)
					if len(raw) >= 2 && raw[0] == '"' && raw[len(raw)-1] == '"' {
						return raw[1 : len(raw)-1]
					}
					return raw
				}
			}
		}
	}
	return ""
}
