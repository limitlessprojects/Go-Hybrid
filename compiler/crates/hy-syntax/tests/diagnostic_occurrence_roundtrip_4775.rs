use hy_diag::{resolve_diagnostic_jump, stamp_diagnostics_for_source, ByteSpan, JumpError};
use hy_syntax::parse_file;

#[test]
fn parser_diagnostic_after_multibyte_comment_selects_second_occurrence() {
    let src = "package main\nfn main() {\n\t/*\u{96ea}\u{1f600}e\u{0301}*/ v := 1 + Bad\n}\n";
    let mut diags = parse_file("t.hy", src).expect_err("Bad violates LX2 identifier case");
    stamp_diagnostics_for_source(src, &mut diags);

    let bad_offset = src.find("Bad").expect("Bad present in source");
    let jump = resolve_diagnostic_jump(&diags[0], src, Some(1), Some(1)).expect("jump resolves");
    assert_eq!(
        jump.byte_span,
        ByteSpan {
            start: bad_offset,
            end: bad_offset + "Bad".len(),
        }
    );
    assert_eq!(&src[jump.byte_span.start..jump.byte_span.end], "Bad");

    let edited = src.replacen("v := 1", "v := 10", 1);
    assert_eq!(
        resolve_diagnostic_jump(&diags[0], &edited, Some(1), Some(1)),
        Err(JumpError::StaleSnapshot)
    );
}
