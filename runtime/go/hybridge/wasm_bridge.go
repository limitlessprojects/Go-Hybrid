package hybridge

import (
	"context"

	"github.com/tetratelabs/wazero/api"
)

//go:generate go run ../../../tools/wasm2go/main.go

type WasmBridge struct{}

var globalBridge = &WasmBridge{}

func GetWasmBridge() (*WasmBridge, error) {
	return globalBridge, nil
}

func (b *WasmBridge) Memory() api.Memory {
	return genMemInstance
}

func (b *WasmBridge) Context() context.Context {
	return context.Background()
}
