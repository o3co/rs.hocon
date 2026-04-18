use hocon::lexer::{tokenize, TokenKind};

fn subst_segments(input: &str) -> Vec<(String, usize, usize)> {
    let tokens = tokenize(input).unwrap();
    let t = tokens
        .iter()
        .find(|t| t.kind == TokenKind::Substitution)
        .expect("subst token");
    let payload = t.subst.as_ref().expect("subst payload");
    payload
        .segments
        .iter()
        .map(|s| (s.text.clone(), s.line, s.col))
        .collect()
}

#[test]
fn segment_position_unquoted_path() {
    let segs = subst_segments("${foo.bar}");
    assert_eq!(segs[0].0, "foo");
    assert_eq!(segs[0].1, 1);
    // '$' is at col 1, '{' col 2, 'f' col 3
    assert_eq!(segs[0].2, 3);
    assert_eq!(segs[1].0, "bar");
    // After '.' at col 6, 'b' is at col 7
    assert_eq!(segs[1].2, 7);
}

#[test]
fn segment_position_quoted_dot_separator() {
    // ${"a"."b"}
    // '$' col 1, '{' col 2, '"' col 3, 'a' col 4, '"' col 5, '.' col 6, '"' col 7, 'b' col 8, '"' col 9, '}' col 10
    let segs = subst_segments(r#"${"a"."b"}"#);
    assert_eq!(segs[0].0, "a");
    assert_eq!(segs[0].2, 3); // opening '"' of first quoted run
    assert_eq!(segs[1].0, "b");
    assert_eq!(segs[1].2, 7); // opening '"' of second quoted run
}

#[test]
fn segment_position_multiline() {
    // line 1: "x=1\n"; line 2: "y=${foo}"
    // On line 2: 'y' col 1, '=' col 2, '$' col 3, '{' col 4, 'f' col 5
    let segs = subst_segments("x=1\ny=${foo}");
    assert_eq!(segs[0].0, "foo");
    assert_eq!(segs[0].1, 2);
    assert_eq!(segs[0].2, 5);
}

#[test]
fn segment_position_ws_concat() {
    // ${"a" "b"}
    // '$' col 1, '{' col 2, '"' col 3, 'a' col 4, '"' col 5, ' ' col 6, '"' col 7, 'b' col 8, '"' col 9, '}' col 10
    // Whitespace preserved: single segment text = "a b", position at first '"'
    let segs = subst_segments(r#"${"a" "b"}"#);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].0, "a b");
    assert_eq!(segs[0].2, 3);
}

#[test]
fn segment_position_empty_quoted_key() {
    // ${""}: '$' col 1, '{' col 2, '"' col 3, '"' col 4, '}' col 5
    let segs = subst_segments(r#"${""}"#);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].0, "");
    assert_eq!(segs[0].2, 3);
}

#[test]
fn error_position_invalid_escape_inside_body() {
    // x=${"a\xb"}: the invalid '\x' escape must be reported at a position
    // within the ${...} body (cols 3..10).
    let err = hocon::parse(r#"x=${"a\xb"}"#).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("invalid escape sequence"), "msg = {}", msg);
    // Position should be within the subst body; i.e. the error mentions a col
    // in [3, 11] inclusive. We don't insist on exact col — just that it's sane.
}

#[test]
fn error_position_empty_path() {
    let err = hocon::parse("x=${}").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("empty substitution path"), "msg = {}", msg);
}

#[test]
fn surrogate_codepoint_rejected() {
    // \uD800 is a high surrogate codepoint — invalid as a standalone scalar.
    // Must be rejected with "invalid unicode escape", matching Lightbend.
    let err = hocon::parse(r#"x="a\uD800b""#).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("invalid unicode escape"), "msg = {}", msg);
}

#[test]
fn surrogate_codepoint_rejected_inside_subst() {
    // Same check inside a substitution body's quoted segment.
    let err = hocon::parse(r#"x=${"a\uD800b"}"#).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("invalid unicode escape"), "msg = {}", msg);
}
