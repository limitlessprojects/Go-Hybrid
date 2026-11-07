//! Tests for curated foreign method surfaces of `time.time` / `time.Time`.
//! Guarantees that calling phantom/unsupported methods rejects fail-closed with HYB-E1002 (Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_time_time_methods_are_rejected() {
    let bad = check(
        "import \"time\"\n\
         struct stats {\n\
             t time.time\n\
             n int\n\
         }\n\
         fn main() {\n\
             var s stats\n\
             s.t.totally_fake()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(
        bad.verdict,
        Verdict::Reject,
        "expected reject, got {:?}",
        bad.diagnostics
    );
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
    let diag = bad
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        diag.message.contains("time.time"),
        "message should name time.time: {}",
        diag.message
    );
    assert!(
        diag.message.contains("totally_fake"),
        "message should name the missing method: {}",
        diag.message
    );
}

#[test]
fn pointer_time_time_phantom_method_is_rejected() {
    let bad = check(
        "import \"time\"\n\
         fn inspect(t *time.time) {\n\
             t.phantom_call()\n\
         }\n",
    );
    assert_eq!(
        bad.verdict,
        Verdict::Reject,
        "expected reject, got {:?}",
        bad.diagnostics
    );
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
    let diag = bad
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        diag.message.contains("time.time"),
        "message: {}",
        diag.message
    );
    assert!(
        diag.message.contains("phantom_call"),
        "message: {}",
        diag.message
    );
}

#[test]
fn package_level_time_functions_called_as_methods_are_rejected() {
    // time.now, time.since, time.until, and time.sleep are package-level functions, NOT methods on time.Time
    let bad_now = check(
        "import \"time\"\n\
         fn inspect(t *time.time) {\n\
             t.now()\n\
         }\n",
    );
    assert_eq!(bad_now.verdict, Verdict::Reject);
    assert!(codes(&bad_now).iter().any(|c| c == "HYB-E1002"));

    let bad_since = check(
        "import \"time\"\n\
         fn inspect(t *time.time) {\n\
             t.since()\n\
         }\n",
    );
    assert_eq!(bad_since.verdict, Verdict::Reject);
    assert!(codes(&bad_since).iter().any(|c| c == "HYB-E1002"));

    let bad_sleep = check(
        "import \"time\"\n\
         fn inspect(t *time.time) {\n\
             t.sleep()\n\
         }\n",
    );
    assert_eq!(bad_sleep.verdict, Verdict::Reject);
    assert!(codes(&bad_sleep).iter().any(|c| c == "HYB-E1002"));
}

#[test]
fn real_time_time_methods_accept() {
    let good = check(
        "import \"time\"\n\
         struct stats {\n\
             t time.time\n\
         }\n\
         fn run(s *stats) {\n\
             s.t.is_zero()\n\
             s.t.unix()\n\
             s.t.utc()\n\
             s.t.local()\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "expected accept, got diagnostics: {:?}",
        good.diagnostics
    );
}

#[test]
fn all_real_pointer_time_time_methods_accept() {
    let good = check(
        "import \"time\"\n\
         fn inspect(t *time.time) {\n\
             t.add()\n\
             t.add_date()\n\
             t.after()\n\
             t.append_binary()\n\
             t.append_format()\n\
             t.append_text()\n\
             t.before()\n\
             t.compare()\n\
             t.day()\n\
             t.equal()\n\
             t.format()\n\
             t.go_string()\n\
             t.gob_decode()\n\
             t.gob_encode()\n\
             t.hour()\n\
             t.in()\n\
             t.is_dst()\n\
             t.is_zero()\n\
             t.local()\n\
             t.location()\n\
             t.marshal_binary()\n\
             t.marshal_json()\n\
             t.marshal_text()\n\
             t.minute()\n\
             t.month()\n\
             t.nanosecond()\n\
             t.round()\n\
             t.second()\n\
             t.string()\n\
             t.sub()\n\
             t.truncate()\n\
             t.unmarshal_binary()\n\
             t.unmarshal_json()\n\
             t.unmarshal_text()\n\
             t.utc()\n\
             t.unix()\n\
             t.unix_micro()\n\
             t.unix_milli()\n\
             t.unix_nano()\n\
             t.weekday()\n\
             t.year()\n\
             t.year_day()\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "expected accept, got diagnostics: {:?}",
        good.diagnostics
    );
}
