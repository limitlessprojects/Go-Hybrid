package hybridge

import (
	"context"
	_ "embed"
	"fmt"
	"sync"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
)

//go:embed hyrt.wasm
var hyrtWasmBytes []byte

type WasmBridge struct {
	mu      sync.Mutex
	ctx     context.Context
	runtime wazero.Runtime
	mod     api.Module

	fnSha3Digest256      api.Function
	fnSha3StreamCheck    api.Function
	fnSha2256            api.Function
	fnFlateCompress      api.Function
	fnFlateDecompress    api.Function
	fnEd25519Sign        api.Function
	fnEd25519Verify      api.Function
	fnNop                api.Function
	fnJsonParse          api.Function
	fnJsonNewObject      api.Function
	fnJsonNewArray       api.Function
	fnJsonGetArray       api.Function
	fnJsonGetInt         api.Function
	fnJsonGetString      api.Function
	fnJsonArrayLen       api.Function
	fnJsonArrayGet       api.Function
	fnJsonArrayPushStr   api.Function
	fnJsonSet            api.Function
	fnJsonValueText      api.Function
	fnJsonToStringPretty api.Function
	fnJsonRelease        api.Function
	fnJsonLiveCount      api.Function
	fnRegexIsMatch       api.Function
	fnRegexIsValid       api.Function
	fnAbiWord            api.Function
	fnAlloc              api.Function
	fnFree               api.Function
}

var stackPool = sync.Pool{
	New: func() any {
		s := make([]uint64, 8)
		return &s
	},
}

func getStack() *[]uint64 {
	return stackPool.Get().(*[]uint64)
}

func releaseStack(s *[]uint64) {
	stackPool.Put(s)
}

var (
	globalBridge *WasmBridge
	bridgeOnce   sync.Once
	bridgeErr    error
)

func GetWasmBridge() (*WasmBridge, error) {
	bridgeOnce.Do(func() {
		ctx := context.Background()
		r := wazero.NewRuntime(ctx)

		compiled, err := r.CompileModule(ctx, hyrtWasmBytes)
		if err != nil {
			bridgeErr = fmt.Errorf("failed to compile hyrt.wasm: %w", err)
			return
		}

		mod, err := r.InstantiateModule(ctx, compiled, wazero.NewModuleConfig().WithSysNanotime())
		if err != nil {
			bridgeErr = fmt.Errorf("failed to instantiate hyrt.wasm: %w", err)
			return
		}

		b := &WasmBridge{
			ctx:     ctx,
			runtime: r,
			mod:     mod,

			fnSha3Digest256:      mod.ExportedFunction("hy_sha3_digest256"),
			fnSha3StreamCheck:    mod.ExportedFunction("hy_sha3_stream_checksum"),
			fnSha2256:            mod.ExportedFunction("hy_sha2_256"),
			fnFlateCompress:      mod.ExportedFunction("hy_flate_compress"),
			fnFlateDecompress:    mod.ExportedFunction("hy_flate_decompress"),
			fnEd25519Sign:        mod.ExportedFunction("hy_ed25519_sign"),
			fnEd25519Verify:      mod.ExportedFunction("hy_ed25519_verify"),
			fnNop:                mod.ExportedFunction("hy_nop"),
			fnJsonParse:          mod.ExportedFunction("hy_json_parse"),
			fnJsonNewObject:      mod.ExportedFunction("hy_json_new_object"),
			fnJsonNewArray:       mod.ExportedFunction("hy_json_new_array"),
			fnJsonGetArray:       mod.ExportedFunction("hy_json_get_array"),
			fnJsonGetInt:         mod.ExportedFunction("hy_json_get_int"),
			fnJsonGetString:      mod.ExportedFunction("hy_json_get_string"),
			fnJsonArrayLen:       mod.ExportedFunction("hy_json_array_len"),
			fnJsonArrayGet:       mod.ExportedFunction("hy_json_array_get"),
			fnJsonArrayPushStr:   mod.ExportedFunction("hy_json_array_push_string"),
			fnJsonSet:            mod.ExportedFunction("hy_json_set"),
			fnJsonValueText:      mod.ExportedFunction("hy_json_value_text"),
			fnJsonToStringPretty: mod.ExportedFunction("hy_json_to_string_pretty"),
			fnJsonRelease:        mod.ExportedFunction("hy_json_release"),
			fnJsonLiveCount:      mod.ExportedFunction("hy_json_live_count"),
			fnRegexIsMatch:       mod.ExportedFunction("hy_regex_is_match"),
			fnRegexIsValid:       mod.ExportedFunction("hy_regex_is_valid"),
			fnAbiWord:            mod.ExportedFunction("hyrt_abi_word"),
			fnAlloc:              mod.ExportedFunction("hyrt_alloc"),
			fnFree:               mod.ExportedFunction("hyrt_free"),
		}
		globalBridge = b
	})
	return globalBridge, bridgeErr
}

func (b *WasmBridge) Memory() api.Memory {
	return b.mod.Memory()
}

func (b *WasmBridge) Context() context.Context {
	return b.ctx
}

func (b *WasmBridge) Alloc(size uint32) (uint32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	if b.fnAlloc == nil {
		return 0, fmt.Errorf("hyrt_alloc export not found")
	}
	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(size)
	if err := b.fnAlloc.CallWithStack(b.ctx, stack); err != nil {
		return 0, err
	}
	return uint32(stack[0]), nil
}

func (b *WasmBridge) Free(ptr uint32, size uint32) {
	b.mu.Lock()
	defer b.mu.Unlock()

	if b.fnFree != nil {
		sp := getStack()
		defer releaseStack(sp)
		stack := *sp
		stack[0] = uint64(ptr)
		stack[1] = uint64(size)
		_ = b.fnFree.CallWithStack(b.ctx, stack)
	}
}

func (b *WasmBridge) HySha3Digest256(dataPtr, dataLen, outPtr uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(dataPtr)
	stack[1] = uint64(dataLen)
	stack[2] = uint64(outPtr)
	if err := b.fnSha3Digest256.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HySha3StreamChecksum(iters, seed uint64) (uint64, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = iters
	stack[1] = seed
	if err := b.fnSha3StreamCheck.CallWithStack(b.ctx, stack); err != nil {
		return 0, err
	}
	return stack[0], nil
}

func (b *WasmBridge) HySha2256(dataPtr, dataLen, outPtr uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(dataPtr)
	stack[1] = uint64(dataLen)
	stack[2] = uint64(outPtr)
	if err := b.fnSha2256.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyFlateCompress(dataPtr, dataLen, outPtr, capPtr, outLenPtr uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(dataPtr)
	stack[1] = uint64(dataLen)
	stack[2] = uint64(outPtr)
	stack[3] = uint64(capPtr)
	stack[4] = uint64(outLenPtr)
	if err := b.fnFlateCompress.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyFlateDecompress(dataPtr, dataLen, outPtr, capPtr, outLenPtr uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(dataPtr)
	stack[1] = uint64(dataLen)
	stack[2] = uint64(outPtr)
	stack[3] = uint64(capPtr)
	stack[4] = uint64(outLenPtr)
	if err := b.fnFlateDecompress.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyEd25519Sign(secretPtr, msgPtr, msgLen, outSigPtr uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(secretPtr)
	stack[1] = uint64(msgPtr)
	stack[2] = uint64(msgLen)
	stack[3] = uint64(outSigPtr)
	if err := b.fnEd25519Sign.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyEd25519Verify(pubkeyPtr, msgPtr, msgLen, sigPtr uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(pubkeyPtr)
	stack[1] = uint64(msgPtr)
	stack[2] = uint64(msgLen)
	stack[3] = uint64(sigPtr)
	if err := b.fnEd25519Verify.CallWithStack(b.ctx, stack); err != nil {
		return 0, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyNop(x uint64) uint64 {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = x
	if err := b.fnNop.CallWithStack(b.ctx, stack); err != nil {
		return 0
	}
	return stack[0]
}

func (b *WasmBridge) HyJsonParse(ptr, len, outHandle uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(ptr)
	stack[1] = uint64(len)
	stack[2] = uint64(outHandle)
	if err := b.fnJsonParse.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonNewObject(outHandle uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(outHandle)
	if err := b.fnJsonNewObject.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonNewArray(outHandle uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(outHandle)
	if err := b.fnJsonNewArray.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonGetArray(v uint64, kptr, klen, outHandle uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = v
	stack[1] = uint64(kptr)
	stack[2] = uint64(klen)
	stack[3] = uint64(outHandle)
	if err := b.fnJsonGetArray.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonGetInt(v uint64, kptr, klen, out uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = v
	stack[1] = uint64(kptr)
	stack[2] = uint64(klen)
	stack[3] = uint64(out)
	if err := b.fnJsonGetInt.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonGetString(v uint64, kptr, klen, outHandle uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = v
	stack[1] = uint64(kptr)
	stack[2] = uint64(klen)
	stack[3] = uint64(outHandle)
	if err := b.fnJsonGetString.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonArrayLen(arr uint64, out uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = arr
	stack[1] = uint64(out)
	if err := b.fnJsonArrayLen.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonArrayGet(arr uint64, i uint64, outHandle uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = arr
	stack[1] = i
	stack[2] = uint64(outHandle)
	if err := b.fnJsonArrayGet.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonArrayPushString(arr uint64, sptr, slen uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = arr
	stack[1] = uint64(sptr)
	stack[2] = uint64(slen)
	if err := b.fnJsonArrayPushStr.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonSet(v uint64, kptr, klen uint32, value uint64) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = v
	stack[1] = uint64(kptr)
	stack[2] = uint64(klen)
	stack[3] = value
	if err := b.fnJsonSet.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonValueText(v uint64, out, capVal, outLen uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = v
	stack[1] = uint64(out)
	stack[2] = uint64(capVal)
	stack[3] = uint64(outLen)
	if err := b.fnJsonValueText.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonToStringPretty(v uint64, out, capVal, outLen uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = v
	stack[1] = uint64(out)
	stack[2] = uint64(capVal)
	stack[3] = uint64(outLen)
	if err := b.fnJsonToStringPretty.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonRelease(h uint64) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = h
	if err := b.fnJsonRelease.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyJsonLiveCount(out uint32) (int32, error) {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(out)
	if err := b.fnJsonLiveCount.CallWithStack(b.ctx, stack); err != nil {
		return 1, err
	}
	return int32(stack[0]), nil
}

func (b *WasmBridge) HyRegexIsMatch(pptr, plen, tptr, tlen uint32) int64 {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(pptr)
	stack[1] = uint64(plen)
	stack[2] = uint64(tptr)
	stack[3] = uint64(tlen)
	if err := b.fnRegexIsMatch.CallWithStack(b.ctx, stack); err != nil {
		return -1
	}
	return int64(stack[0])
}

func (b *WasmBridge) HyRegexIsValid(pptr, plen uint32) int64 {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	stack[0] = uint64(pptr)
	stack[1] = uint64(plen)
	if err := b.fnRegexIsValid.CallWithStack(b.ctx, stack); err != nil {
		return 0
	}
	return int64(stack[0])
}

func (b *WasmBridge) HyAbiWord() uint64 {
	b.mu.Lock()
	defer b.mu.Unlock()

	sp := getStack()
	defer releaseStack(sp)
	stack := *sp
	if err := b.fnAbiWord.CallWithStack(b.ctx, stack); err != nil {
		return 0
	}
	return stack[0]
}
