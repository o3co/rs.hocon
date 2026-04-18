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

/// Parse `ParseError at L:C: ...` / `ResolveError at L:C: ...` display and
/// return (line, col). Panics if the format doesn't match.
fn parse_err_pos(msg: &str) -> (usize, usize) {
    // Look for the first "at L:C:" in the rendered message.
    let at = msg.find(" at ").expect("no ' at ' in error msg");
    let rest = &msg[at + 4..];
    let colon = rest.find(':').expect("no first colon after line");
    let line: usize = rest[..colon].parse().expect("line not a number");
    let rest2 = &rest[colon + 1..];
    let colon2 = rest2.find(':').expect("no second colon after col");
    let col: usize = rest2[..colon2].parse().expect("col not a number");
    (line, col)
}

#[test]
fn error_position_invalid_escape_inside_body() {
    // x=${"a\xb"}
    //  ^1234567890 1
    // The '\' of the invalid escape is at col 7. We assert the reported
    // position lies inside the ${...} body (cols 3..=11).
    let err = hocon::parse(r#"x=${"a\xb"}"#).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("invalid escape sequence"), "msg = {}", msg);
    let (line, col) = parse_err_pos(&msg);
    assert_eq!(line, 1, "line should be 1, got {} (msg = {})", line, msg);
    assert!(
        (3..=11).contains(&col),
        "col {} not in subst body [3, 11] (msg = {})",
        col,
        msg
    );
}

#[test]
fn error_position_empty_path() {
    let err = hocon::parse("x=${}").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("empty substitution path"), "msg = {}", msg);
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
