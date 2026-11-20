//! Regression tests for interprocedural, closure, method, associated/generic,
//! and indirect-call A2 (`O7` + `O11`, `HYB-E3101`) boundary-handle loan conflicts.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult { check_file(&parse_file("test.hy", src).expect("snippet must parse")) }

fn assert_e3101(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected HYB-E3101 for:\n{src}\ngot: {:?}", r.diagnostics);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3101"), "expected HYB-E3101 for:\n{src}\ngot: {:?}", r.diagnostics);
}

#[test]
fn interprocedural_helpers_generic_and_assoc_fns_reject_e3101() {
    for src in [
        "import \"rust/serde_json\"\nfn wipe(d serde_json.value) error {\n d.set(\"users\", true)?\n return nil\n}\nfn main() {\n doc := serde_json.parse(`{\"users\":[{\"name\":\"ada\"}]}`)?\n for u in doc.array(\"users\")? {\n wipe(doc)?\n fmt.println(u.string(\"name\")?)\n }\n}\n",
        "import \"rust/serde_json\"\nfn inner(d serde_json.value) error {\n serde_json.push_string(d, \"x\")?\n return nil\n}\nfn outer(d serde_json.value) error { return inner(d) }\nfn main() {\n arr := serde_json.new_array()?\n v := arr.array(\"items\")?\n outer(arr)?\n fmt.println(len(v))\n}\n",
        "import \"rust/serde_json\"\nfn absorb(dst serde_json.value, src serde_json.value) error {\n dst.set(\"sub\", src)?\n return nil\n}\nfn main() {\n dst := serde_json.new_object()?\n child := serde_json.parse(`{\"items\":[1]}`)?\n items := child.array(\"items\")?\n absorb(dst, child)?\n fmt.println(len(items))\n}\n",
        "import \"rust/serde_json\"\nfn wipe_gen[t any](d serde_json.value, tag t) error {\n _ = tag\n d.set(\"users\", 1)?\n return nil\n}\nfn main() {\n doc := serde_json.parse(`{\"users\":[]}`)?\n v := serde_json.array(doc, \"users\")?\n wipe_gen[int](doc, 7)?\n fmt.println(len(v))\n}\n",
        "import \"rust/serde_json\"\nstruct ops { tag int }\nfn ops.wipe(d serde_json.value) error {\n d.set(\"users\", 1)?\n return nil\n}\nfn main() {\n doc := serde_json.parse(`{\"users\":[]}`)?\n for u in doc.array(\"users\")? {\n ops.wipe(doc)?\n fmt.println(u.int(\"id\")?)\n }\n}\n",
        "import \"rust/serde_json\"\nstruct holder { doc serde_json.value }\nfn wipe_holder(h holder) error {\n h.doc.set(\"users\", 1)?\n return nil\n}\nfn main() {\n doc := serde_json.parse(`{\"users\":[]}`)?\n h := holder{doc: doc}\n v := doc.array(\"users\")?\n wipe_holder(h)?\n fmt.println(len(v))\n}\n",
    ] {
        assert_e3101(src);
    }
}

#[test]
fn closures_methods_and_indirect_calls_reject_e3101() {
    for src in [
        "import \"rust/serde_json\"\nfn main() {\n doc := serde_json.parse(`{\"users\":[{\"name\":\"ada\"}]}`)?\n wipe := fn() error {\n doc.set(\"users\", true)?\n return nil\n }\n w2 := wipe\n for u in doc.array(\"users\")? {\n w2()?\n fmt.println(u.string(\"name\")?)\n }\n}\n",
        "import \"rust/serde_json\"\nfn main() {\n doc := serde_json.parse(`{\"users\":[]}`)?\n wipe := fn(d serde_json.value) error {\n d.set(\"users\", 1)?\n return nil\n }\n v := doc.array(\"users\")?\n wipe(doc)?\n fmt.println(len(v))\n}\n",
        "import \"rust/serde_json\"\nfn main() {\n doc := serde_json.parse(`{\"users\":[]}`)?\n v := doc.array(\"users\")?\n (fn() error {\n doc.set(\"users\", 1)?\n return nil\n })()?\n fmt.println(len(v))\n}\n",
        "import \"rust/serde_json\"\nfn run_cb(cb fn() error) error { return cb() }\nfn main() {\n doc := serde_json.parse(`{\"users\":[]}`)?\n v := doc.array(\"users\")?\n run_cb(fn() error {\n doc.set(\"users\", 1)?\n return nil\n })?\n fmt.println(len(v))\n}\n",
        "import \"rust/serde_json\"\nstruct holder { doc serde_json.value }\nfn (h holder) wipe() error {\n h.doc.set(\"users\", 1)?\n return nil\n}\nfn main() {\n doc := serde_json.parse(`{\"users\":[]}`)?\n h := holder{doc: doc}\n v := doc.array(\"users\")?\n h.wipe()?\n fmt.println(len(v))\n}\n",
        "import \"rust/serde_json\"\nstruct svc { n int }\nfn (s *svc) wipe(d serde_json.value) error {\n s.n = s.n + 1\n d.set(\"users\", 1)?\n return nil\n}\nfn main() {\n var s svc\n doc := serde_json.parse(`{\"users\":[]}`)?\n v := doc.array(\"users\")?\n s.wipe(doc)?\n fmt.println(len(v))\n}\n",
        "import \"rust/serde_json\"\nstruct runner { cb fn(d serde_json.value) error }\nfn drive(doc serde_json.value, cb fn(d serde_json.value) error, r runner) error {\n for u in doc.array(\"users\")? {\n cb(doc)?\n r.cb(doc)?\n fmt.println(u.int(\"id\")?)\n }\n return nil\n}\nfn main() {}\n",
        "import \"rust/serde_json\"\nfn same_doc(d serde_json.value) serde_json.value { return d }\nfn main() {\n doc := serde_json.parse(`{\"users\":[]}`)?\n v := doc.array(\"users\")?\n same_doc(doc).set(\"users\", 1)?\n fmt.println(len(v))\n}\n",
    ] {
        assert_e3101(src);
    }
}

#[test]
fn read_only_callees_shadowed_closures_and_post_window_mutations_accept() {
    let r = check(
        "import \"rust/serde_json\"\n\
         struct holder { doc serde_json.value }\n\
         struct ops { tag int }\n\
         fn count_users(d serde_json.value) (int, error) {\n\
             n := 0\n\
             for u in d.array(\"users\")? {\n\
                 if u.int(\"logins\")? > 0 { n++ }\n\
             }\n\
             return n, nil\n\
         }\n\
         fn ops.peek(d serde_json.value) (string, error) { return serde_json.to_string_pretty(d) }\n\
         fn (h holder) check_len() (int, error) {\n\
             arr := h.doc.array(\"users\")?\n\
             return len(arr), nil\n\
         }\n\
         fn wipe(d serde_json.value) error {\n\
             d.set(\"audited\", true)?\n\
             return nil\n\
         }\n\
         fn run_int(cb fn(v int), x int) { cb(x) }\n\
         fn main() {\n\
             doc := serde_json.parse(`{\"users\":[{\"name\":\"ada\",\"logins\":3}]}`)?\n\
             other := serde_json.new_object()?\n\
             h := holder{doc: doc}\n\
             ro := fn() (int, error) { return count_users(doc) }\n\
             shadow := fn(doc serde_json.value) error {\n\
                 doc.set(\"k\", 1)?\n\
                 return nil\n\
             }\n\
             var late fn() error\n\
             for u in doc.array(\"users\")? {\n\
                 c := count_users(doc)?\n\
                 s := ops.peek(doc)?\n\
                 m := h.check_len()?\n\
                 r := ro()?\n\
                 shadow(other)?\n\
                 run_int(fn(v int) { fmt.println(v) }, u.int(\"logins\")?)\n\
                 late = fn() error {\n\
                     doc.set(\"late\", true)?\n\
                     return nil\n\
                 }\n\
                 fmt.println(c, s, m, r, u.string(\"name\")?)\n\
             }\n\
             wipe(doc)?\n\
             late()?\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected ACCEPT, got: {:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "expected 0 diagnostics, got: {:?}", r.diagnostics);
}
