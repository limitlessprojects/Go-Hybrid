# Slice 2 Remaining Exports - Call-Stack Trace Design & Wiring Plan

## 1. Six-Tier Call-Stack Trace Tree for JSON / Regex / Flate2 / Ed25519

```
Project: Go-Hybrid
├── Folder: runtime/go/jsonbinding
│   └── File: json.go
│       └── Struct: Value
│           └── Method: Parse(jsonStr string) (*Value, error)
│               └── Variable State: jsonStr_len=128
├── Folder: runtime/go/hybridge
│   └── File: ownhandle.go
│       └── Struct: OwnHandle
│           └── Method: Alloc(size int) (ptr uint32, buf []byte, err error)
│               └── Variable State: ptr=0x1000, len=128
├── Folder: runtime/go/hybridge
│   └── File: wasm_bridge.go
│       └── Struct: WasmBridge
│           └── Method: HyJsonParse(ptr, len, outHandle uint32) (int32, error)
│               └── Variable State: outHandle_written=1, status=0
├── Folder: runtime/go/regexbinding
│   └── File: regex.go
│       └── Struct: Matcher
│           └── Method: IsMatch(pat, text string) (bool, error)
│               └── Variable State: pat_len=12, text_len=48, match=true
├── Folder: runtime/go/flatebinding
│   └── File: flate.go
│       └── Method: Compress(data []byte) ([]byte, error)
│           └── Variable State: input_len=1024, compressed_len=256
└── Folder: runtime/go/ed25519binding
    └── File: ed25519.go
        └── Method: Sign(secretKey [32]byte, msg []byte) ([64]byte, error)
            └── Variable State: msg_len=100, signature_valid=true
```

---

## 2. Export Inventory (22 Exports Total)

1. `hy_sha3_digest256` (sha3)
2. `hy_sha3_stream_checksum` (sha3)
3. `hy_sha2_256` (sha2)
4. `hy_flate_compress` (flate2)
5. `hy_flate_decompress` (flate2)
6. `hy_ed25519_sign` (ed25519)
7. `hy_ed25519_verify` (ed25519)
8. `hy_json_parse` (serde_json)
9. `hy_json_new_object` (serde_json)
10. `hy_json_new_array` (serde_json)
11. `hy_json_get_array` (serde_json)
12. `hy_json_get_int` (serde_json)
13. `hy_json_get_string` (serde_json)
14. `hy_json_array_len` (serde_json)
15. `hy_json_array_get` (serde_json)
16. `hy_json_array_push_string` (serde_json)
17. `hy_json_set` (serde_json)
18. `hy_json_value_text` (serde_json)
19. `hy_json_to_string_pretty` (serde_json)
20. `hy_json_release` (serde_json)
21. `hy_json_live_count` (serde_json)
22. `hy_regex_is_match` / `hy_regex_is_valid` (regex)
