use hocon::{tokenize, HoconError, TokenKind};

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

/// Extract `(line, col)` from a `HoconError::Parse` variant.
/// Panics if the error is not a Parse variant.
fn parse_err_pos(err: &HoconError) -> (usize, usize) {
    match err {
        HoconError::Parse(e) => (e.line, e.col),
        other => panic!("expected HoconError::Parse, got {:?}", other),
    }
}

#[test]
fn error_position_invalid_escape_inside_body() {
    // x=${"a\xb"}
    //  ^1234567890 1
    // The '\' of the invalid escape is at col 7. We assert the reported
    // position lies inside the ${...} body (cols 3..=11).
    let err = hocon::parse(r#"x=${"a\xb"}"#).unwrap_err();
    assert!(
        err.to_string().contains("invalid escape sequence"),
        "msg = {}",
        err
    );
    let (line, col) = parse_err_pos(&err);
    assert_eq!(line, 1, "line should be 1, got {} (err = {})", line, err);
    assert!(
        (3..=11).contains(&col),
        "col {} not in subst body [3, 11] (err = {})",
        col,
        err
    );
}

#[test]
fn error_position_empty_path() {
    let err = hocon::parse("x=${}").unwrap_err();
    assert!(
        err.to_string().contains("empty substitution path"),
        "err = {}",
        err
    );
    let (line, _col) = parse_err_pos(&err);
    assert_eq!(line, 1, "line should be 1, got {} (err = {})", line, err);
}

#[test]
fn surrogate_codepoint_rejected() {
    // \uD800 is a high surrogate codepoint, which is not a Unicode scalar value.
    // We intentionally reject it with "invalid unicode escape"; this differs
    // from Lightbend (Java accepts it because java.lang.String is a sequence
    // of 16-bit code units) and follows Rust `char` / Unicode scalar-value
    // constraints. See spec §"QUOTED reading rules" and the surrogate note.
    let err = hocon::parse(r#"x="a\uD800b""#).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("invalid unicode escape"), "msg = {}", msg);
}

#[test]
fn surrogate_codepoint_rejected_inside_subst() {
    // Same intentional Rust-side rejection inside a substitution body.
    // See surrogate_codepoint_rejected for the Lightbend-divergence rationale.
    let err = hocon::parse(r#"x=${"a\uD800b"}"#).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("invalid unicode escape"), "msg = {}", msg);
}
