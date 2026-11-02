// W-024 (Issue #757) sample 01: ACCEPT-MANAGED — OwnHandle (`serde_json.doc`)
// and BorrowHandle (`serde_json.view`) lowered to a pure-Go Wasm linear-memory
// arena (`[]byte` pooled page per `OwnHandle`) with zero `libhyrt.a` or `cgo`.
package main

import (
    "fmt"
    "rust/serde_json"
)

fn extract_service(raw string) (string, error):
    managed("parse and read inside linear-memory arena", "w024-spike"):
        doc := serde_json.parse(raw)?
        defer doc.free()
        root := doc.root()
        return root.get_str("service"), nil

fn main():
    svc, err := extract_service("{\"service\":\"hybrid-compiler\",\"tier\":\"zero-cgo\"}")
    if err != nil:
        fmt.Println("error:", err)
        return
    fmt.Println(svc)
