use hy_diag::{
    byte_offset_to_display_pos, check_output_json, resolve_diagnostic_jump, snapshot_sha256,
    span_to_byte_span, stamp_diagnostics_for_source, ByteSpan, CheckSummary, Diagnostic, JumpError,
    Pos, Span,
};

#[test]
fn sha256_matches_standard_test_vectors() {
    assert_eq!(
        snapshot_sha256(""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        snapshot_sha256("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn duplicate_v_after_multibyte_comment_selects_second_occurrence_22_23() {
    let src = "\t/*\u{96ea}\u{1f600}e\u{0301}*/ v +   v";
    assert_eq!(&src[16..17], "v");
    assert_eq!(&src[22..23], "v");

    let first_span = Span::new(Pos::new(1, 11), Pos::new(1, 12));
    let second_span = Span::new(Pos::new(1, 17), Pos::new(1, 18));

    let first_bytes = span_to_byte_span(src, first_span).expect("first v resolves");
    assert_eq!(first_bytes, ByteSpan { start: 16, end: 17 });
    let first_disp = byte_offset_to_display_pos(src, first_bytes.start).unwrap();
    assert_eq!(
        (first_disp.scalar_col, first_disp.utf16_col, first_disp.utf8_col),
        (11, 12, 17)
    );

    // Stamped envelope path (proposed byte_span + snapshot_sha256).
    let mut diag = Diagnostic::error("HYB-E3001", "dup.hy", second_span, "second occurrence");
    stamp_diagnostics_for_source(src, std::slice::from_mut(&mut diag));
    assert_eq!(diag.byte_span, Some(ByteSpan { start: 22, end: 23 }));

    let stamped_jump = resolve_diagnostic_jump(&diag, src, Some(1), Some(1)).expect("jump succeeds");
    assert_eq!(
        stamped_jump.byte_span,
        ByteSpan { start: 22, end: 23 },
        "must resolve second v at [22, 23), never first v at [16, 17)"
    );
    assert_eq!(
        (
            stamped_jump.display.start.scalar_col,
            stamped_jump.display.start.utf16_col,
            stamped_jump.display.start.utf8_col,
        ),
        (17, 18, 23)
    );
    assert_eq!(&src[stamped_jump.byte_span.start..stamped_jump.byte_span.end], "v");

    // Cheaper adapter path (unstamped Diagnostic relying on scalar converter + doc version).
    let unstamped = Diagnostic::error("HYB-E3001", "dup.hy", second_span, "second occurrence");
    let adapter_jump =
        resolve_diagnostic_jump(&unstamped, src, Some(1), Some(1)).expect("adapter resolves");
    assert_eq!(adapter_jump, stamped_jump);
}

#[test]
fn editing_snapshot_before_jump_triggers_stale_recheck_gate() {
    let original = "\t/*\u{96ea}\u{1f600}e\u{0301}*/ v +   v";
    let second_span = Span::new(Pos::new(1, 17), Pos::new(1, 18));

    let mut stamped = Diagnostic::error("HYB-E3001", "dup.hy", second_span, "second occurrence");
    stamp_diagnostics_for_source(original, std::slice::from_mut(&mut stamped));

    // Insert a character before the target after the diagnostic was emitted.
    let edited = "\t/*\u{96ea}\u{1f600}e\u{0301}*/ x + v +   v";

    // Stamped digest catches stale snapshot even if a CLI caller has no doc version (None, None).
    assert_eq!(
        resolve_diagnostic_jump(&stamped, edited, None, None),
        Err(JumpError::StaleSnapshot)
    );

    // Unstamped scalar adapter requires doc version mismatch to detect staleness; without it,
    // scalar column 17 silently lands on the shifted text instead of rejecting the jump.
    let unstamped = Diagnostic::error("HYB-E3001", "dup.hy", second_span, "second occurrence");
    assert_eq!(
        resolve_diagnostic_jump(&unstamped, edited, Some(1), Some(2)),
        Err(JumpError::StaleSnapshot)
    );
}

#[test]
fn roundtrip_covers_24_fixtures_72_encodings_crlf_and_eof() {
    let fixtures = [
        ("ascii simple", "fn main() { x := 1 }", 1usize, 13usize, 14usize),
        ("leading indent", "    let foo = bar", 1, 9, 12),
        ("cjk prefix", "// \u{4e2d}\u{6587}\nfoo := bar", 2, 1, 4),
        ("emoji prefix", "// \u{1f600}\nalpha + alpha", 2, 9, 14),
        ("combining mark", "// e\u{0301}\nbeta + beta", 2, 8, 12),
        ("dup cjk+emoji", "\t/*\u{96ea}\u{1f600}e\u{0301}*/ v +   v", 1, 17, 18),
        ("crlf line 2", "package main\r\nfn run() {}\r\n", 2, 4, 7),
        ("crlf dup line 2", "line1\r\n/*\u{1f680}*/ id + id\r\n", 2, 12, 14),
        ("crlf line 3", "a\r\nb\r\n\u{00e9} target", 3, 3, 9),
        ("tabs and unicode", "\t\t\u{03b1}\u{03b2} token", 1, 6, 11),
        ("empty line before", "\n\n// ok\nval", 4, 1, 4),
        ("multiple surrogates", "\u{1f600}\u{1f601}\u{1f602} hit", 1, 5, 8),
        ("bmp 3-byte", "\u{20ac}\u{2603}\u{4e00} sym", 1, 5, 8),
        ("latin1 2-byte", "\u{00e0}\u{00e1}\u{00e2} x", 1, 5, 6),
        ("mixed comment", "/*a\u{1f600}b*/ x + x", 1, 11, 12),
        ("multiline comment", "/* line1\n\u{96ea} line2 */ tok", 2, 12, 15),
        ("duplicate on line 3", "p := 1\nq := 2\n\u{1f980} z + z", 3, 7, 8),
        ("trailing unicode", "let v = 1 // \u{1f600}", 1, 5, 6),
        ("zwj emoji sequence", "\u{1f469}\u{200d}\u{1f4bb} dev", 1, 5, 8),
        ("arabic+ascii", "// \u{0645}\u{0631}\nx1 := x1", 2, 1, 3),
        ("hangul+ascii", "// \u{d55c}\u{ae00}\nx2 := x2", 2, 1, 3),
        ("single byte token", "x", 1, 1, 2),
        ("point at line end", "abc", 1, 4, 4),
        ("point at crlf eof", "fn main() {}\r\n", 2, 1, 1),
    ];
    assert_eq!(fixtures.len(), 24);

    for (name, src, line, start_col, end_col) in fixtures {
        let span = Span::new(Pos::new(line as u32, start_col as u32), Pos::new(line as u32, end_col as u32));
        let mut diag = Diagnostic::error("HYB-E3003", "f.hy", span, name);
        stamp_diagnostics_for_source(src, std::slice::from_mut(&mut diag));
        let bytes = diag.byte_span.expect("byte span must resolve");
        let jump = resolve_diagnostic_jump(&diag, src, Some(1), Some(1)).expect("jump resolves");
        assert_eq!(jump.byte_span, bytes, "fixture `{name}`");

        // Verify scalar, UTF-16, and UTF-8 column mappings independently (3 encodings x 24 = 72 checks).
        let line_prefix_start = &src[src[..bytes.start].rfind('\n').map_or(0, |i| i + 1)..bytes.start];
        let expected_scalar = line_prefix_start.chars().count() as u32 + 1;
        let expected_utf16 = line_prefix_start.encode_utf16().count() as u32 + 1;
        let expected_utf8 = line_prefix_start.len() as u32 + 1;
        assert_eq!(jump.display.start.scalar_col, expected_scalar, "scalar mismatch `{name}`");
        assert_eq!(jump.display.start.utf16_col, expected_utf16, "utf16 mismatch `{name}`");
        assert_eq!(jump.display.start.utf8_col, expected_utf8, "utf8 mismatch `{name}`");
    }

    let src = "fn f() {}\n";
    let mut diag = Diagnostic::error("HYB-E3003", "f.hy", Span::point(Pos::new(2, 1)), "eof");
    stamp_diagnostics_for_source(src, std::slice::from_mut(&mut diag));
    let summary = CheckSummary {
        file: "f.hy".into(),
        verdict: "reject".into(),
        errors: 1,
        warnings: 0,
        managed: None,
        snapshot_sha256: Some(snapshot_sha256(src)),
    };
    let json = check_output_json(&[diag], &summary);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["diagnostics"][0]["byte_span"]["start"], 10);
    assert_eq!(parsed["diagnostics"][0]["byte_span"]["end"], 10);
    assert_eq!(
        parsed["diagnostics"][0]["snapshot_sha256"],
        parsed["summary"]["snapshot_sha256"]
    );
}
