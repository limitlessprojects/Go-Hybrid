//! foreign_mutator_const_write.rs - regression tests for HYB-E3006 (T22
//! deep immutability on `const`) through FOREIGN in-place mutators, from
//! soundness issue #4530: the `package_mutated_arg` descriptor table in
//! foreign.rs modelled only `sort.*` and three `io` functions, and method
//! calls consulted no mutator model at all, so `hex.decode(dst, src)` and
//! `reader.read(dst)` wrote into `const` slices with 0 diagnostics - and
//! two tasks decoding into one file-level `const` raced for real
//! (`go build -race`: write/write inside `encoding/hex.Decode`).
//!
//! The fix extends the package table (hex.decode, io.copy / io.copy_n,
//! the strconv append family), adds a `method_mutated_arg` table keyed by
//! the receiver's boundary type name, and types the reader/writer/buffer
//! constructor results so method calls on fresh handles reach the tables.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_e3006(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for {}, got {:?}",
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accepted(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for {}, got {:?}",
        what,
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Rejects - package-level mutators into a `const` root
// ---------------------------------------------------------------------------

#[test]
fn hex_decode_into_local_const_is_e3006() {
    // Issue #4530's minimal shape: the const is silently rewritten to "hi".
    assert_e3006(
        "import \"encoding/hex\"\n\
         fn main() {\n\
         \x20   const dst = []byte{0, 0}\n\
         \x20   src := []byte(\"6869\")\n\
         \x20   n, err := hex.decode(dst, src)\n\
         \x20   _ = n\n\
         \x20   _ = err\n\
         \x20   fmt.println(\"{dst[0]} {dst[1]}\")\n\
         }\n",
        "hex.decode into a local const slice",
    );
}

#[test]
fn hex_decode_into_file_const_field_is_e3006() {
    // Deep immutability: the destination is a field under a file-level
    // const root, so the write is ambient package-state mutation.
    assert_e3006(
        "import \"encoding/hex\"\n\
         struct bag {\n\
         \x20   data []byte\n\
         }\n\
         const global_bag = bag{data: []byte{0, 0}}\n\
         fn main() {\n\
         \x20   src := []byte(\"6869\")\n\
         \x20   n, err := hex.decode(global_bag.data, src)\n\
         \x20   _ = n\n\
         \x20   _ = err\n\
         \x20   fmt.println(\"{global_bag.data[0]}\")\n\
         }\n",
        "hex.decode into a file-level const struct field",
    );
}

#[test]
fn hex_decode_into_shared_file_const_from_tasks_is_e3006() {
    // The race shape: two tasks decode into ONE file-level const slice.
    assert_e3006(
        "import \"encoding/hex\"\n\
         const cells = []byte{0, 0}\n\
         fn bump(done chan<- bool) {\n\
         \x20   src := []byte(\"6869\")\n\
         \x20   for i in 0..2000 {\n\
         \x20       n, err := hex.decode(cells, src)\n\
         \x20       _ = n\n\
         \x20       _ = err\n\
         \x20   }\n\
         \x20   done <- true\n\
         }\n\
         fn main() {\n\
         \x20   done := make(chan bool, 2)\n\
         \x20   task bump(done)\n\
         \x20   task bump(done)\n\
         \x20   <-done\n\
         \x20   <-done\n\
         \x20   fmt.println(\"{cells[0]}\")\n\
         }\n",
        "hex.decode into a shared file-level const from two tasks",
    );
}

#[test]
fn io_copy_into_const_is_e3006() {
    assert_e3006(
        "import \"io\"\n\
         import \"strings\"\n\
         fn main() {\n\
         \x20   const dst = []byte{0, 0}\n\
         \x20   r := strings.new_reader(\"hi\")\n\
         \x20   n, err := io.copy(dst, r)\n\
         \x20   _ = n\n\
         \x20   _ = err\n\
         }\n",
        "io.copy into a const destination",
    );
}

#[test]
fn strconv_append_int_on_const_is_e3006() {
    // Append-style functions write into the destination's backing array
    // whenever it has spare capacity.
    assert_e3006(
        "import \"strconv\"\n\
         fn main() {\n\
         \x20   const dst = []byte{0, 0, 0, 0, 0, 0, 0, 0}\n\
         \x20   out := strconv.append_int(dst, 42, 10)\n\
         \x20   fmt.println(\"{out[0]}\")\n\
         }\n",
        "strconv.append_int on a const slice",
    );
}

#[test]
fn io_read_full_into_const_stays_e3006() {
    // Control: the pre-existing table entry keeps rejecting - the rule
    // was always meant to cover this shape.
    assert_e3006(
        "import \"io\"\n\
         import \"strings\"\n\
         fn main() {\n\
         \x20   const dst = []byte{0, 0}\n\
         \x20   r := strings.new_reader(\"hi\")\n\
         \x20   n, err := io.read_full(r, dst)\n\
         \x20   _ = n\n\
         \x20   _ = err\n\
         }\n",
        "io.read_full into a const slice (pre-existing control)",
    );
}

// ---------------------------------------------------------------------------
// Rejects - method-call mutators into a `const` root
// ---------------------------------------------------------------------------

#[test]
fn strings_reader_read_into_const_is_e3006() {
    assert_e3006(
        "import \"strings\"\n\
         fn main() {\n\
         \x20   const dst = []byte{0, 0, 0, 0, 0}\n\
         \x20   r := strings.new_reader(\"hello\")\n\
         \x20   n, err := r.read(dst)\n\
         \x20   _ = n\n\
         \x20   _ = err\n\
         \x20   fmt.println(\"{dst[0]}\")\n\
         }\n",
        "strings.reader read into a const slice",
    );
}

#[test]
fn bufio_reader_read_into_const_is_e3006() {
    assert_e3006(
        "import \"bufio\"\n\
         import \"strings\"\n\
         fn main() {\n\
         \x20   const dst = []byte{0, 0, 0, 0, 0}\n\
         \x20   r := bufio.new_reader(strings.new_reader(\"hello\"))\n\
         \x20   n, err := r.read(dst)\n\
         \x20   _ = n\n\
         \x20   _ = err\n\
         \x20   fmt.println(\"{dst[0]}\")\n\
         }\n",
        "bufio.reader read into a const slice",
    );
}

#[test]
fn declared_reader_read_into_file_const_is_e3006() {
    // The receiver is a declared `strings.reader` parameter rather than a
    // fresh constructor result - the boundary type name resolves either
    // way, so the mutator table applies.
    assert_e3006(
        "import \"strings\"\n\
         const dst = []byte{0, 0}\n\
         fn sink(r strings.reader) {\n\
         \x20   n, err := r.read(dst)\n\
         \x20   _ = n\n\
         \x20   _ = err\n\
         }\n\
         fn main() {\n\
         \x20   sink(strings.new_reader(\"hi\"))\n\
         \x20   fmt.println(\"{dst[0]}\")\n\
         }\n",
        "declared strings.reader read into a file-level const",
    );
}

// ---------------------------------------------------------------------------
// Accepts - the same calls on writable destinations, and pure calls on const
// ---------------------------------------------------------------------------

#[test]
fn hex_decode_into_var_is_accepted() {
    assert_accepted(
        "import \"encoding/hex\"\n\
         fn main() {\n\
         \x20   dst := []byte{0, 0}\n\
         \x20   src := []byte(\"6869\")\n\
         \x20   n, err := hex.decode(dst, src)\n\
         \x20   _ = n\n\
         \x20   _ = err\n\
         \x20   fmt.println(\"{dst[0]}\")\n\
         }\n",
        "hex.decode into a var slice",
    );
}

#[test]
fn reader_read_into_var_is_accepted() {
    assert_accepted(
        "import \"strings\"\n\
         fn main() {\n\
         \x20   dst := []byte{0, 0}\n\
         \x20   r := strings.new_reader(\"hi\")\n\
         \x20   n, err := r.read(dst)\n\
         \x20   _ = n\n\
         \x20   _ = err\n\
         \x20   fmt.println(\"{dst[0]}\")\n\
         }\n",
        "strings.reader read into a var slice",
    );
}

#[test]
fn hex_encode_on_const_src_is_accepted() {
    // The binding lowers `hex.encode` to the pure `EncodeToString(src)`:
    // it writes nothing, so a const source stays legal. (The dst-form
    // `Encode(dst, src)` is not part of the Hybrid surface.)
    assert_accepted(
        "import \"encoding/hex\"\n\
         const src = []byte{104, 105}\n\
         fn main() {\n\
         \x20   s := hex.encode(src)\n\
         \x20   fmt.println(s)\n\
         }\n",
        "pure hex.encode on a const source",
    );
}

#[test]
fn hex_decode_string_on_const_is_accepted() {
    assert_accepted(
        "import \"encoding/hex\"\n\
         fn main() {\n\
         \x20   const s = \"6869\"\n\
         \x20   out, err := hex.decode_string(s)\n\
         \x20   _ = err\n\
         \x20   fmt.println(\"{out[0]}\")\n\
         }\n",
        "hex.decode_string on a const string",
    );
}

#[test]
fn strconv_append_int_on_var_is_accepted() {
    assert_accepted(
        "import \"strconv\"\n\
         fn main() {\n\
         \x20   dst := []byte{}\n\
         \x20   out := strconv.append_int(dst, 42, 10)\n\
         \x20   fmt.println(\"{out[0]}\")\n\
         }\n",
        "strconv.append_int on a var slice",
    );
}
