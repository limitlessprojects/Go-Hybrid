# Native hook admission contract

This is a real native Go `-toolexec` hook and a deliberately small, opt-in analysis experiment. A `//hybrid:checked` function admits concrete scalar values, channels of scalars, direct calls to other admitted functions, and a small set of builtins. It rejects reference-bearing parameters/results/locals, closures, mutable global access, selectors/foreign calls, address escapes, indirect calls, and unmarked callees. Unmarked Go functions are passed through and carry **no** Hybrid safety claim. This is not a port of the complete Rust ownership checker, an affine-token system, a full go/analysis implementation, or an assertion about Kubernetes/etcd.

Compiler `-importcfg` export data feeds `go/types`; source imports are not resolved through an unrelated GOPATH. Version probes include a hash of the wrapper binary so edited analysis does not silently reuse stale cached approvals. Compiler/assembler/linker invocations and tool exit codes are forwarded.

Build with pinned Go 1.26.9: `go build -o nativehook.exe ./nativehook` (omit `.exe` on Linux). Then use an absolute wrapper path: `go build -toolexec=ABSOLUTE_NATIVEHOOK ./nativehook/testdata/pass` prints 42. `testdata/reject` must fail with HYB-NATIVE-E001. These are experimental diagnostics, not registered language error codes.
