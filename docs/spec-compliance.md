# HOCON Spec Compliance — rs.hocon

This file extends the canonical checklist at
[`xx.hocon/docs/spec-checklist.md`](https://github.com/o3co/xx.hocon/blob/main/docs/spec-checklist.md)
with rs.hocon-specific status for all 209 items, in the same order and with the
same item descriptions verbatim.

- **`tests:`** records the test path (or fixture) that exercises each item, or `—` if no test covers it.
- **`status:`** glyphs and compliance-rate formula follow the legend and
  convention defined in the template. Summary: `✅` pass, `⚠️` partial,
  `❌` fail/known violation, `🤷` unverified, `➖` out of scope.
- Compliance rate: `(✅ + ⚠️·0.5) / total` (spec-total) and
  `(✅ + ⚠️·0.5) / (total − ➖)` (in-scope). See template for full formula.
- Section headings match the template (S1–S26 with sub-sections).
- Cross-implementation roll-up: [`xx.hocon/docs/compliance-matrix.md`](https://github.com/o3co/xx.hocon/blob/main/docs/compliance-matrix.md).

---

## S1. Unchanged from JSON

- **S1.1** Files must be valid UTF-8 — §Unchanged from JSON (L117)
  tests: tests/testdata/hocon/bom.conf (fixture)
  status: ✅
- **S1.2.1** Quoted strings accept valid JSON escape sequences (`\" \\ \/ \b \f \n \r \t`) — §Unchanged from JSON (L118)
  tests: src/lexer.rs:758 (handles_escape_sequences); tests/testdata/hocon/subst-tokenize/st10-escape-newline.conf (fixture); tests/testdata/hocon/subst-tokenize/st11-escape-tab.conf (fixture); tests/testdata/hocon/subst-tokenize/st12-escape-backslash.conf (fixture); tests/testdata/hocon/subst-tokenize/st13-escape-quote.conf (fixture); tests/testdata/hocon/subst-tokenize/st19-quoted-escape-slash.conf (fixture); tests/testdata/hocon/subst-tokenize/st20-quoted-escape-backspace-formfeed.conf (fixture)
  status: ✅
- **S1.2.2** Unknown / invalid escape sequence (e.g. `\q`, `\x`) is rejected — §Unchanged from JSON (L118)
  tests: tests/integration_test.rs:407 (test_unknown_escape_sequence_error); tests/integration_test.rs:413 (test_unknown_escape_a_error); tests/testdata/hocon/subst-tokenize/st-err01-invalid-escape-x.conf (fixture); tests/testdata/hocon/subst-tokenize/st-err02-invalid-escape-q.conf (fixture)
  status: ✅
- **S1.2.3** Malformed `\uXXXX` (short / non-hex) is rejected — §Unchanged from JSON (L118)
  tests: tests/testdata/hocon/subst-tokenize/st-err03-invalid-unicode-short.conf (fixture); tests/testdata/hocon/subst-tokenize/st-err04-invalid-unicode-nonhex.conf (fixture)
  status: ✅
- **S1.2.4** Unescaped control char / raw newline in quoted string is rejected — §Unchanged from JSON (L118)
  tests: tests/testdata/hocon/subst-tokenize/st-err07-newline-in-string.conf (fixture)
  status: ✅
- **S1.2.5** Unterminated quoted string is rejected — §Unchanged from JSON (L118)
  tests: src/lexer.rs:862 (throws_on_unterminated_string); tests/testdata/hocon/subst-tokenize/st-err06-unterminated-string.conf (fixture)
  status: ✅
- **S1.2.6** Unpaired UTF-16 surrogate codepoint in `\uXXXX` escape — §Unchanged from JSON (L118)
  out-of-scope: intentional language-natural divergence. Java (Lightbend reference) silently accepts unpaired surrogates because Java strings are 16-bit code-unit sequences; Rust `char` and Go `rune` cannot represent them and reject. xx.hocon conformance fixtures cannot cover this case (the Java generator fails to encode unpaired surrogates as UTF-8 when writing expected JSON). Each implementation follows its language's string-type constraints. Documented in xx.hocon commit 86bd82e.
  tests: tests/lexer_test.rs:113 (surrogate_codepoint_rejected); tests/lexer_test.rs:125 (surrogate_codepoint_rejected_inside_subst)
  status: ➖
- **S1.3** Value types: string, number, object, array, boolean, null — §Unchanged from JSON (L119)
  tests: src/parser.rs:674 (parses_boolean_and_null); src/parser.rs:697 (parses_integer_scalars); tests/integration_test.rs:12 (parse_simple_config)
  status: ✅
- **S1.4** Number formats match JSON (no NaN, no Infinity) — §Unchanged from JSON (L120)
  tests: src/parser.rs:708 (parses_float_scalars); src/parser.rs:719 (dot_prefix_is_string_not_number)
  status: ✅

## S2. Comments

- **S2.1** `//` line comment — §Comments (L125)
  tests: src/lexer.rs:735 (skips_slash_comments_keeps_newline); tests/integration_test.rs:91 (parse_with_comments); tests/testdata/hocon/equiv01/comments.conf (fixture)
  status: ✅
- **S2.2** `#` line comment — §Comments (L125)
  tests: src/lexer.rs:743 (skips_hash_comments); tests/integration_test.rs:91 (parse_with_comments); tests/testdata/hocon/equiv01/comments.conf (fixture)
  status: ✅
- **S2.3** Comment markers inside quoted strings are literal — §Comments (L126)
  tests: src/lexer.rs:895 (s2_3_comment_markers_inside_quoted_string_are_literal); tests/integration_test.rs:521 (s2_3_comment_markers_in_quoted_values_are_literal)
  status: ✅

## S3. Omit root braces

- **S3.1** Empty file is invalid — §Omit root braces (L130)
  tests: tests/spec_s3_1_empty_file.rs (s3_1_1_empty_string, s3_1_2_whitespace_only, s3_1_3_newlines_only, s3_1_4_comment_only, s3_1_5_bom_only, s3_1_6_mixed_ws_comment); tests/conformance_empty_file.rs (ef01-ef06); src/parser.rs (parses_empty_input — updated to expect Err via hocon::parse)
  status: ✅ (was ⚠️; guard added at parse_with_env/parse_file_with_env entry — Phase 6 #3h)
- **S3.2** Root non-object/non-array is invalid (when explicitly enclosed) — §Omit root braces (L131)
  tests: tests/integration_test.rs:651 (s3_2_root_bare_string_rejected)
  status: ✅
- **S3.3** Implicit `{}` when file does not start with `[` or `{` — §Omit root braces (L134)
  tests: tests/testdata/hocon/equiv01/no-root-braces.conf (fixture); tests/integration_test.rs:12 (parse_simple_config)
  status: ✅
- **S3.4** Unbalanced trailing `}` without opening `{` is invalid — §Omit root braces (L138)
  tests: tests/integration_test.rs:277 (test_stray_brace_after_root)
  status: ✅

## S4. Key-value separator

- **S4.1** `=` is interchangeable with `:` — §Key-value separator (L143)
  tests: src/parser.rs:638 (parses_key_colon_value); tests/testdata/hocon/equiv01/equals.conf (fixture)
  status: ✅
- **S4.2** `:` / `=` may be omitted before `{` — §Key-value separator (L146)
  tests: tests/testdata/hocon/equiv01/omit-colons.conf (fixture)
  status: ✅

## S5. Commas

- **S5.1** Newline acts as element/field separator — §Commas (L152)
  tests: tests/testdata/hocon/equiv01/no-commas.conf (fixture)
  status: ✅
- **S5.2** Single trailing comma is allowed and ignored — §Commas (L155)
  tests: tests/integration_test.rs:535 (s5_2_single_trailing_comma_in_array_allowed); tests/integration_test.rs:547 (s5_2_single_trailing_comma_in_object_allowed)
  status: ✅
- **S5.3** Two trailing commas (`[1,2,3,,]`) is invalid — §Commas (L160)
  tests: tests/integration_test.rs:560 (s5_3_two_trailing_commas_in_array_rejected); tests/integration_test.rs:568 (s5_3_two_trailing_commas_in_object_rejected)
  status: ✅
- **S5.4** Leading comma (`[,1,2,3]`) is invalid — §Commas (L161)
  tests: tests/integration_test.rs:582 (s5_4_leading_comma_in_array_rejected); tests/integration_test.rs:590 (s5_4_leading_comma_in_object_rejected)
  status: ✅
- **S5.5** Two consecutive commas (`[1,,2,3]`) is invalid — §Commas (L162)
  tests: tests/integration_test.rs:601 (s5_5_two_consecutive_commas_in_array_rejected)
  status: ✅
- **S5.6** Same comma rules apply to object fields — §Commas (L163)
  tests: tests/integration_test.rs:547 (s5_2_single_trailing_comma_in_object_allowed); tests/integration_test.rs:568 (s5_3_two_trailing_commas_in_object_rejected); tests/integration_test.rs:590 (s5_4_leading_comma_in_object_rejected); tests/integration_test.rs:614 (s5_6_two_consecutive_commas_between_object_fields_rejected)
  status: ✅

## S6. Whitespace

- **S6.1** Unicode Zs/Zl/Zp category characters are whitespace — §Whitespace (L170)
  tests: src/lexer.rs:954 (s6_1_em_space_separates_tokens_spec); src/lexer.rs:967 (s6_1_line_separator_separates_tokens_spec)
  status: ✅ (fixed in #84, [#62](https://github.com/o3co/rs.hocon/issues/62)) — is_hocon_whitespace covers all Zs/Zl/Zp members
- **S6.2** Non-breaking spaces (0x00A0, 0x2007, 0x202F) are whitespace — §Whitespace (L171)
  tests: src/lexer.rs:984 (s6_2_nbsp_separates_tokens_spec); src/lexer.rs:997 (s6_2_figure_space_separates_tokens_spec); src/lexer.rs:1010 (s6_2_narrow_nbsp_separates_tokens_spec)
  status: ✅ (fixed in #84, [#62](https://github.com/o3co/rs.hocon/issues/62)) — NBSP variants included in is_hocon_whitespace
- **S6.3** BOM (0xFEFF) treated as whitespace — §Whitespace (L173)
  tests: src/lexer.rs:887 (strips_utf8_bom); src/lexer.rs:1123 (s6_3_bom_midstream_is_whitespace); tests/testdata/hocon/bom.conf (fixture)
  status: ✅ (broadened in #84) — BOM now treated as whitespace anywhere, not just at start of input
- **S6.4** ASCII control whitespace (tab, vtab, FF, CR, FS, GS, RS, US) — §Whitespace (L174)
  tests: src/lexer.rs:1027 (s6_4_tab_is_whitespace); src/lexer.rs:1040 (s6_4_cr_is_whitespace); src/lexer.rs:1055 (s6_4_vtab_is_whitespace_spec); src/lexer.rs:1068 (s6_4_ff_is_whitespace_spec); src/lexer.rs:1083 (s6_4_fs_gs_rs_us_are_whitespace_spec)
  status: ✅ (fixed in #84, [#62](https://github.com/o3co/rs.hocon/issues/62)) — all 8 ASCII control whitespace chars covered by is_hocon_whitespace
- **S6.5** "newline" means specifically 0x000A (LF) — §Whitespace (L183)
  tests: src/lexer.rs:855 (tokenizes_newlines)
  status: ✅

## S7. Duplicate keys and object merging

- **S7.1** Later non-object key overrides earlier — §Duplicate keys (L189)
  tests: src/resolver/mod.rs:84 (last_value_wins_for_scalars)
  status: ✅
- **S7.2** Two object values are merged recursively — §Duplicate keys (L191)
  tests: src/resolver/mod.rs:73 (merges_duplicate_object_keys); tests/integration_test.rs:64 (parse_with_deep_merge)
  status: ✅
- **S7.3** Merge: fields in only one object are kept — §Duplicate keys (L199)
  tests: src/resolver/mod.rs:73 (merges_duplicate_object_keys); tests/integration_test.rs:64 (parse_with_deep_merge)
  status: ✅
- **S7.4** Merge: non-object field in both → second wins — §Duplicate keys (L201)
  tests: src/resolver/mod.rs:84 (last_value_wins_for_scalars)
  status: ✅
- **S7.5** Merge: object field in both → recursive merge — §Duplicate keys (L203)
  tests: tests/integration_test.rs:261 (test_object_concat_deep_merge); tests/integration_test.rs:269 (test_object_concat_deep_merge_multiple)
  status: ✅
- **S7.6** Intermediate non-object value breaks merge with later object — §Duplicate keys (L207)
  tests: src/resolver/mod.rs:211 (resolves_last_assignment_wins_for_substitution)
  status: ✅

## S8. Unquoted strings

- **S8.1** Forbidden characters rejected (``$ " { } [ ] : = , + # ` ^ ? ! @ * & \``) and whitespace — §Unquoted strings (L245)
  tests: tests/integration_test.rs:441 (unquoted_forbids_spec_special_chars)
  status: ✅
- **S8.2** `//` inside an unquoted string starts a comment — §Unquoted strings (L248)
  tests: src/lexer.rs:735 (skips_slash_comments_keeps_newline)
  status: ✅
- **S8.3** Initial token `true`/`false`/`null` parsed as keyword — §Unquoted strings (L250)
  tests: src/parser.rs:674 (parses_boolean_and_null); tests/testdata/hocon/equiv01/original.json (fixture)
  status: ✅
- **S8.4** Initial number characters parse as number — §Unquoted strings (L250)
  tests: src/lexer.rs:792 (tokenizes_numbers_as_unquoted); src/parser.rs:697 (parses_integer_scalars)
  status: ✅
- **S8.5** Embedded `true`/`false`/`null`/number become string content — §Unquoted strings (L266)
  tests: tests/testdata/hocon/equiv01/unquoted.conf (fixture)
  status: ✅
- **S8.6** Unquoted string cannot begin with `0-9` or `-` — §Unquoted strings (L270) *(spec wording is verbatim; the implementation follows the E8 / Lightbend reading — see status note below)*
  tests: tests/s8_unquoted_starts.rs (xx.hocon fixtures: 29 success — us01–us14, us16, us17–us30 — and 1 known gap — us15; plus 4 path-rule regressions and 9 E8 explicit assertions); src/lexer.rs (E8 value-start positive tests for `-foo` and `123abc`)
  status: ✅ (post-E8 amendment) — xx.hocon E8 rewritten 2026-05-20 ([xx.hocon#31](https://github.com/o3co/xx.hocon/issues/31) / commit `dd102e8`) adopts Lightbend's pragmatic reading of HOCON.md L270-276 ("begin" = value-position begin, not token-position). rs.hocon now matches:
  - Value-start `-` not followed by a digit → unquoted text (was lex error). RFC 8259 JSON-number requires a digit after `-`; bare `-` / `-foo` therefore fall outside L270's disallow scope. Lightbend reference produces `{"a":"-foo"}` / `{"a":"-"}`.
  - Value-start digit-leading runs → coerced via i64-first then f64-fallback at the value layer (e.g. `a = 01` → `{"a": 1}` with normalized `raw = "1"`). rs.hocon has no separate `Number` token kind, so coercion happens at resolve time on the unquoted token in `parse_scalar_value` (src/parser.rs). F3 `01` BREAKING for `get_string("a")` callers (was `"01"` pre-E8, now `"1"`); typed getters and JSON serialization unaffected. The number-coercion entry gate also requires `-` to be followed by a digit, so Rust-specific tokens like `-inf` / `-nan` (which `f64::parse` accepts but Lightbend's `parseDouble` rejects) stay on the string path.
  - Concat-continuation after value-tokens (e.g. `b = ${a}-bar` → `"foo-bar"`) now accepted — the strict `-` reject at the main tokenize loop's unquoted-start branch has been removed (src/lexer.rs).
  - `+` rejection retained in both value-start and concat-continuation positions (HOCON `+=` operator reservation) — `+` is excluded from `is_unquoted_start`, so it cannot open a value or extend a prior token in concat position. The lexer hits the catch-all "unexpected character" branch.
  - Path-element strict checks preserved (out of E8 scope): `parse_subst_body`'s segment-start `-` check (src/lexer.rs) and the per-segment check in `parse_key` (src/parser.rs) — these police path-element composition, not value-position unquoted strings. Tests `${-foo}` and `a.-foo = 1` still throw `ParseError`.
  - Known gap: us15 `a = 1e+x` concerns the `+` reservation enforced mid-token at the value-parser layer in Lightbend — a separate spec rule from S8.6's *begin-character* constraint. Lightbend errors here; rs.hocon's `is_unquoted_continue` only excludes `+` when followed by `=`, so `1e+x` lexes as a single unquoted token and falls back to a string scalar. Tracked as `#[should_panic]` tripwire in the conformance file.
- **S8.7** No escape sequences in unquoted strings — §Unquoted strings (L253)
  tests: src/lexer.rs:1181 (s8_7_backslash_is_rejected_in_unquoted_context)
  status: ✅
- **S8.8** Unquoted strings allow control characters except forbidden set — §Unquoted strings (L280)
  tests: src/lexer.rs:1196 (s8_8_soh_allowed_in_unquoted_string); src/lexer.rs:1208 (s8_8_bel_allowed_in_unquoted_string)
  status: ✅

## S9. Multi-line strings

- **S9.1** `"""..."""` triple-quoted string — §Multi-line strings (L291)
  tests: src/lexer.rs:770 (tokenizes_triple_quoted_strings); tests/integration_test.rs:105 (parse_with_triple_quoted_string); tests/testdata/hocon/equiv05/triple-quotes.conf (fixture)
  status: ✅
- **S9.2** Newlines and whitespace preserved literally — §Multi-line strings (L293)
  tests: src/lexer.rs:770 (tokenizes_triple_quoted_strings); tests/testdata/hocon/equiv05/triple-quotes.conf (fixture)
  status: ✅
- **S9.3** Unicode escapes NOT interpreted inside triple-quoted — §Multi-line strings (L294)
  tests: tests/testdata/hocon/equiv05/triple-quotes.conf (fixture)
  status: ✅
- **S9.4** Scala-style trailing extra quotes are part of string — §Multi-line strings (L300)
  tests: tests/testdata/hocon/equiv05/triple-quotes.conf (fixture)
  status: ✅
- **S9.5** Unterminated `"""` raises an error — §Multi-line strings (L291-293, by analogy with quoted strings)
  tests: src/lexer.rs:872 (throws_on_unterminated_triple_quoted_string); tests/integration_test.rs:501 (test_unterminated_triple_quoted_string_errors)
  status: ✅

## S10. Value concatenation

- **S10.1** Simple values + non-newline whitespace → string concat — §Value concatenation (L310)
  tests: src/resolver/mod.rs:221 (resolves_string_concat_with_substitution); tests/integration_test.rs:34 (parse_with_substitutions); tests/testdata/hocon/equiv01/unquoted.conf (fixture)
  status: ✅
- **S10.2** All arrays → array concatenation — §Value concatenation (L312)
  tests: tests/spec_phase5.rs (s10_2_spec_array_concat)
  status: ✅
- **S10.3** All objects → object merge (concatenation) — §Value concatenation (L314)
  tests: tests/integration_test.rs:261 (test_object_concat_deep_merge); tests/integration_test.rs:158 (test_braced_root_object_concat)
  status: ✅
- **S10.4** Mixing arrays + objects in concat is an error — §Array and object concatenation (L385)
  tests: tests/integration_test.rs (s10_4_array_object_concat_is_error, s10_4_subst_obj_plus_array_is_error, s10_4_empty_edge_array_plus_empty_object_is_error, s10_4_optional_missing_mid_concat_is_error, s10_4_optional_missing_only_piece_ok, s10_4_numeric_obj_concat_still_works); tests/concat_errors_test.rs (ce01, ce02, ce07, ce08, ce10, ce11, ce14, ce15)
  status: ✅ (fixed in Phase 6 #3b, closes #65)
- **S10.5** Inner whitespace between simple values preserved — §String value concatenation (L332)
  tests: tests/testdata/hocon/equiv01/unquoted.conf (fixture)
  status: ✅
- **S10.6** Leading/trailing whitespace around concat discarded — §String value concatenation (L346)
  tests: tests/testdata/hocon/equiv01/unquoted.conf (fixture)
  status: ✅
- **S10.7** Concatenation does not span a newline — §String value concatenation (L335)
  tests: tests/integration_test.rs:689 (s10_7_concat_does_not_span_newline)
  status: ✅
- **S10.8** String concat allowed in field keys — §Value concatenation (L317)
  tests: tests/integration_test.rs (s10_8_* it blocks: quoted, basic, three-token, dotted-prefix concat, dotted-tail concat, quoted+unquoted, inline-object shorthand, leading-dot + S11.1 interaction, tab whitespace)
  status: ✅ — fixed in #66; `parse_key` accepts space-concat continuations and merges into the LAST segment with literal space. Leading '.' after whitespace stays a path separator per S11.1.
- **S10.9** `true`/`false` stringify to `"true"`/`"false"` in concat — §String value concatenation (L363)
  tests: tests/testdata/hocon/equiv01/unquoted.conf (fixture)
  status: ✅
- **S10.10** `null` stringifies to `"null"` in concat — §String value concatenation (L364)
  tests: tests/testdata/hocon/equiv01/unquoted.conf (fixture)
  status: ✅
- **S10.11** Numbers stringify as written in the source file — §String value concatenation (L366)
  tests: tests/testdata/hocon/equiv01/unquoted.conf (fixture)
  status: ✅
- **S10.12** A single non-string value is NOT stringified (type preserved) — §String value concatenation (L376)
  tests: src/parser.rs:674 (parses_boolean_and_null); src/parser.rs:697 (parses_integer_scalars)
  status: ✅
- **S10.13** Array/object appearing in string concat is an error — §String value concatenation (L373)
  tests: tests/integration_test.rs (s10_13_scalar_object_concat_is_error, s10_13_scalar_array_concat_is_error, s10_13_subst_resolved_array_plus_scalar_is_error, s10_13_subst_resolved_object_plus_scalar_is_error); tests/concat_errors_test.rs (ce03, ce04, ce05, ce06, ce12, ce13)
  status: ✅ (fixed in Phase 6 #3b, closes #67)
- **S10.14** Whitespace around obj/array substitutions is ignored — §Concatenation with whitespace (L440)
  tests: tests/integration_test.rs:786 (s10_14_whitespace_around_obj_subst_ignored); tests/integration_test.rs:798 (s10_14_whitespace_around_arr_subst_ignored)
  status: ✅
- **S10.15** Quoted whitespace between obj/array substitutions is an error — §Concatenation with whitespace (L442)
  tests: tests/spec_phase5.rs (s10_15_quoted_ws_between_obj_substs_is_error; s10_15_quoted_ws_between_arr_substs_is_error)
  status: ✅ (incidentally fixed by Phase 6 #3b S10.13 tightening: scalar between structured operands now errors via join_pair type-mismatch)
- **S10.16** Non-newline whitespace in arrays is concat, not separator — §Arrays without commas or newlines (L447)
  tests: tests/testdata/hocon/equiv01/no-commas.conf (fixture)
  status: ✅
- **S10.17** Substitution resolving to an array participates in array concat (`${arr} [x]`) — §Array and object concatenation (L387)
  tests: tests/spec_phase5.rs (s10_17_spec_subst_array_concat)
  status: ✅
- **S10.18** Substitution resolving to an object participates in object merge (`${obj} {x:1}`) — §Array and object concatenation (L388)
  tests: src/resolver/mod.rs:248 (delayed_merge_object_with_substitution)
  status: ✅
- **S10.19** Mixing a substitution-resolved object with a literal array (or vice versa) is an error — §Array and object concatenation (L385-389)
  tests: tests/integration_test.rs (s10_19_subst_obj_concat_literal_array_is_error, s10_19_subst_arr_concat_literal_obj_is_error); tests/concat_errors_test.rs (ce07, ce08)
  status: ✅ (fixed in Phase 6 #3b, closes #68)

## S11. Path expressions

- **S11.1** `.` outside quoted is a path separator — §Path expressions (L483)
  tests: src/parser.rs:644 (parses_dot_notation_keys); tests/integration_test.rs:144 (parse_dot_notation)
  status: ✅
- **S11.2** `.` inside quoted is literal — §Path expressions (L484)
  tests: src/parser.rs:650 (does_not_split_quoted_keys); tests/integration_test.rs:182 (test_quoted_path_lookup)
  status: ✅
- **S11.3** Numbers retain original string representation in paths — §Path expressions (L489)
  tests: tests/lightbend_test.rs:334 (lightbend_test11_numeric_string_keys); tests/lightbend_test.rs:348 (lightbend_test12_long_numeric_keys)
  status: ✅
- **S11.4** `10.0foo` → path `[10, 0foo]` — §Path expressions (L496)
  tests: tests/integration_test.rs:849 (s11_4_numeric_dot_unquoted_path)
  status: ✅
- **S11.5** `foo10.0` → path `[foo10, 0]` — §Path expressions (L498)
  tests: tests/integration_test.rs:866 (s11_5_unquoted_dot_numeric_path)
  status: ✅
- **S11.6** Empty path element must be quoted (`a."".b` ok) — §Path expressions (L515)
  tests: tests/lightbend_test.rs:200 (lightbend_test02_empty_keys_and_quoted_paths)
  status: ✅
- **S11.7** `a..b` and paths starting/ending with `.` are errors — §Path expressions (L517)
  tests: tests/testdata/hocon/subst-tokenize/st-err09-empty-segment-leading-dot.conf (fixture); tests/testdata/hocon/subst-tokenize/st-err10-empty-segment-trailing-dot.conf (fixture); tests/testdata/hocon/subst-tokenize/st-err11-empty-segment-double-dot.conf (fixture)
  status: ✅
- **S11.8** Path expression always stringifies (single `true` → `"true"`) — §Path expressions (L504)
  tests: tests/integration_test.rs:884 (s11_8_path_expression_stringifies_boolean); tests/integration_test.rs:894 (s11_8_path_expression_stringifies_number)
  status: ✅
- **S11.9** Substitutions not allowed inside path expressions — §Path expressions (L479)
  tests: tests/integration_test.rs:908 (s11_9_subst_in_key_rejected)
  status: ✅
- **S11.10** Quoted path segments respected in getter API (e.g. `config.get("foo.\"bar.baz\"")`) — §Path expressions (L485)
  tests: tests/integration_test.rs:182 (test_quoted_path_lookup); tests/integration_test.rs:189 (test_nested_quoted_path_lookup); tests/lightbend_test.rs:200 (lightbend_test02_empty_keys_and_quoted_paths)
  status: ✅

## S12. Paths as keys

- **S12.1** `foo.bar : 42` expands to `foo { bar : 42 }` — §Paths as keys (L530)
  tests: src/parser.rs:644 (parses_dot_notation_keys); tests/integration_test.rs:144 (parse_dot_notation); tests/testdata/hocon/equiv01/path-keys.conf (fixture)
  status: ✅
- **S12.2** Multi-element keys expand to nested objects — §Paths as keys (L538)
  tests: tests/integration_test.rs:19 (parse_nested_config); tests/testdata/hocon/equiv02/path-keys.conf (fixture)
  status: ✅
- **S12.3** Path keys merge per duplicate-key rules — §Paths as keys (L544)
  tests: tests/testdata/hocon/equiv02/path-keys.conf (fixture)
  status: ✅
- **S12.4** Whitespace in keys: `a b c : 42` = `"a b c" : 42` — §Paths as keys (L553)
  tests: tests/testdata/hocon/equiv02/path-keys-weird-whitespace.conf (fixture)
  status: ✅
- **S12.5** `include` may NOT begin a path expression in a key — §Paths as keys (L570)
  tests: tests/integration_test.rs (s12_5_include_as_key_spec); tests/include_reservation_test.rs (ir01–ir14, ir03/ir04 per-impl overrides); src/parser.rs (include_dot_key_is_parse_error, include_nested_object_body_is_parse_error, quoted_include_bypasses_reservation, et al.)
  status: ✅ (fixed in fix/s12.5-include-reservation, closes #71)

## S13. Substitutions

- **S13.1** `${path}` is a required substitution — §Substitutions (L579)
  tests: src/lexer.rs:799 (tokenizes_substitutions); src/parser.rs:730 (parses_substitutions); src/resolver/mod.rs:130 (resolves_substitution)
  status: ✅
- **S13.2** `${?path}` is an optional substitution — §Substitutions (L579)
  tests: src/lexer.rs:806 (tokenizes_optional_substitutions); src/parser.rs:745 (parses_optional_substitutions); src/resolver/mod.rs:148 (resolves_optional_substitution_exists)
  status: ✅
- **S13.3** `${?` is exactly 3 chars (no whitespace before `?`) — §Substitutions (L584)
  tests: tests/integration_test.rs:979 (s13_3_space_before_question_differs_from_optional)
  status: ✅
- **S13.4** Resolver MAY consult external sources (env vars, system properties) for unresolved substitutions — §Substitutions (L588) (concrete env behavior → S26)
  tests: src/resolver/mod.rs:190 (resolves_env_var_fallback); tests/integration_test.rs:46 (parse_with_env_fallback)
  status: ✅
- **S13.5** Substitutions are NOT parsed inside quoted strings — §Substitutions (L593)
  tests: tests/integration_test.rs:1000 (s13_5_no_subst_in_quoted_string)
  status: ✅
- **S13.6** Substitution paths are absolute (rooted at config root) — §Substitutions (L603)
  tests: src/resolver/mod.rs:139 (resolves_nested_path_substitution); tests/testdata/hocon/equiv01/substitutions.conf (fixture)
  status: ✅
- **S13.7** Substitution resolution is last step (can look forward) — §Substitutions (L607)
  tests: src/resolver/mod.rs:239 (resolves_forward_reference)
  status: ✅
- **S13.8** Substitution sees the latest-assigned (merged) value — §Substitutions (L612)
  tests: src/resolver/mod.rs:211 (resolves_last_assignment_wins_for_substitution); tests/lightbend_test.rs:275 (lightbend_test06_delayed_merge)
  status: ✅
- **S13.9** `null` in config blocks env var lookup — §Substitutions (L618)
  tests: tests/integration_test.rs:1014 (s13_9_null_blocks_env_var_lookup_pin); tests/integration_test.rs:1028 (s13_9_null_blocks_env_var_lookup_spec)
  status: ❌ (see #74)
- **S13.10** Required substitution undefined → error — §Substitutions (L627)
  tests: src/resolver/mod.rs:183 (throws_on_unresolved_mandatory); tests/integration_test.rs:470 (resolve_error_is_hocon_error_resolve_variant); tests/testdata/hocon/cycle-expected-error.json (fixture)
  status: ✅
- **S13.11** Optional undefined in field value → field not created — §Substitutions (L632)
  tests: src/resolver/mod.rs:157 (drops_field_for_optional_missing); tests/testdata/hocon/equiv04/missing-substitutions.conf (fixture)
  status: ✅
- **S13.12** Optional undefined in array element → element not added — §Substitutions (L635)
  tests: tests/testdata/hocon/equiv04/missing-substitutions.conf (fixture)
  status: ✅
- **S13.13** Optional undefined in string concat → empty string — §Substitutions (L636)
  tests: tests/integration_test.rs:1041 (s13_13_optional_undefined_in_string_concat_is_empty)
  status: ✅
- **S13.14** Optional undefined in obj/array concat → empty obj/array — §Substitutions (L637)
  tests: tests/integration_test.rs:1054 (s13_14_optional_undefined_in_array_concat_pin); tests/integration_test.rs:1066 (s13_14_optional_undefined_in_array_concat_spec); tests/integration_test.rs:1078 (s13_14_optional_undefined_in_object_concat)
  status: ⚠️ (see #75) — array case broken (whitespace artefacts leak as extra elements); object case ✅
- **S13.15** `foo : ${?bar}${?baz}` skipped only when BOTH undefined — §Substitutions (L640)
  tests: tests/spec_phase5.rs (s13_15_pin_both_optional_undefined_field_exists; s13_15_spec_both_optional_undefined_field_absent [#ignore]; s13_15_one_defined_field_is_created)
  status: ❌
  notes: when both `${?bar}` and `${?baz}` are undefined, `foo` is still created with a null value instead of being dropped. The single-defined sub-case (`bar=hello, foo=${?bar}${?baz}`) correctly produces `foo=hello`.
- **S13.16** Substitutions only in field values / array elements — §Substitutions (L644)
  tests: tests/integration_test.rs:1087 (s13_16_substitution_in_key_is_rejected)
  status: ✅
- **S13.17** Single-substitution value preserves type — §Substitutions (L648)
  tests: src/resolver/mod.rs:93 (resolves_arrays); src/resolver/mod.rs:68 (resolves_nested_objects)
  status: ✅
- **S13.18** Substitution in multi-value concat becomes string — §Substitutions (L650)
  tests: src/resolver/mod.rs:221 (resolves_string_concat_with_substitution); tests/integration_test.rs:34 (parse_with_substitutions)
  status: ✅
- **S13.19** Unterminated `${...}` (missing closing `}`) is rejected — §Substitutions syntax requires closing `}` (L579)
  tests: src/lexer.rs:867 (throws_on_unterminated_substitution); tests/testdata/hocon/subst-tokenize/st-err05-unterminated-subst.conf (fixture)
  status: ✅

### S13a. Self-referential substitutions

- **S13a.1** `path : ${path}` resolves to prior `path` value — §Self-Referential (L666)
  tests: src/resolver/mod.rs:201 (resolves_self_referential_substitution); tests/integration_test.rs:150 (parse_self_referential_substitution)
  status: ✅
- **S13a.2** Self-ref to overridden field works in merge — §Self-Referential (L748)
  tests: tests/lightbend_test.rs:365 (lightbend_test13_substitution_override)
  status: ✅
- **S13a.3** Self-ref before any prior value → undefined → error — §Self-Referential (L767)
  tests: tests/lightbend_test.rs:397 (lightbend_test13_bad_substitution)
  status: ✅
- **S13a.4** Optional self-ref `${?foo}` disappears silently — §Self-Referential (L776)
  tests: src/resolver/mod.rs:157 (drops_field_for_optional_missing)
  status: ✅
- **S13a.5** Substitution hidden by later non-object → no error — §Self-Referential (L780)
  tests: src/resolver/mod.rs:163 (falls_back_to_prior_value)
  status: ✅
- **S13a.6** Cycle inside object `a : { b : ${a} }` → error — §Self-Referential (L688)
  tests: src/resolver/mod.rs:232 (throws_on_circular_substitution); tests/testdata/hocon/cycle.conf (fixture)
  status: ✅
- **S13a.7** Cycle inside array `a : [${a}]` → error — §Self-Referential (L689)
  tests: src/resolver/mod.rs:232 (throws_on_circular_substitution)
  status: ✅
- **S13a.8** Two-step cycle `bar : ${foo}; foo : ${bar}` → error — §Self-Referential (L857)
  tests: src/resolver/mod.rs:232 (throws_on_circular_substitution)
  status: ✅
- **S13a.9** Multi-step cycle `a→b→c→a` → error — §Self-Referential (L862)
  tests: tests/spec_phase5.rs (s13a_9_multi_step_cycle_is_error)
  status: ✅
- **S13a.10** Substitution memoized by instance, not by path — §Self-Referential (L885)
  tests: tests/spec_phase5.rs (s13a_10_memoization_same_value)
  status: ✅
  notes: the spec-observable outcome is that `a` and `b` must end up equal in the ambiguous `a=1,b=2,a=${b},b=${a}` case. Both end up as 2, satisfying the constraint. Internal memoization is not directly observable from the parse API.
- **S13a.11** Object can refer to its own descendant (`bar : { foo : 42, baz : ${bar.foo} }`) — §Self-Referential (L806)
  tests: tests/lightbend_test.rs:303 (lightbend_test09_delayed_merge_object); tests/lightbend_test.rs:316 (lightbend_test10_nested_include)
  status: ✅
- **S13a.12** Self-ref in path expression `${foo.a}` resolves to "below" — §Self-Referential (L791)
  tests: tests/lightbend_test.rs:275 (lightbend_test06_delayed_merge)
  status: ✅
- **S13a.13** `a = ${?a}foo` resolves to `"foo"` (look-back undefined) — §Self-Referential (L841)
  tests: tests/integration_test.rs (s13a_13_optional_self_ref_concat_with_no_prior_spec); tests/self_ref_lookback_test.rs (sr01–sr11)
  status: ✅ (fixed in #76; cleared in cluster phase6-3f)
- **S13a.14** Mutually-referring object fields (`bar.a = ${foo.d}; foo.c = ${bar.b}`) resolve lazily without false cycle — §Self-Referential (L825-834)
  tests: tests/spec_phase5.rs (s13a_14_mutual_refs_no_false_cycle)
  status: ✅

### S13b. `+=` field separator

- **S13b.1** `a += b` expands to `a = ${?a} [b]` — §`+=` field separator (L725)
  tests: src/resolver/mod.rs:103 (handles_plus_equals_on_existing_array); tests/integration_test.rs:84 (parse_with_plus_equals)
  status: ✅
- **S13b.2** `+=` on non-array prior value → error — §`+=` field separator (L732)
  tests: tests/integration_test.rs:941 (s13b_2_plus_eq_on_non_array_pin); tests/integration_test.rs:956 (s13b_2_plus_eq_on_non_array_spec)
  status: ❌ (see #72)
- **S13b.3** `+=` works on first mention of key (no prior `=`) — §`+=` field separator (L734)
  tests: src/resolver/mod.rs:113 (handles_plus_equals_on_missing_key)
  status: ✅

### S13c. List values from environment variables

- **S13c.1** `${X[]}` looks up `X_0`, `X_1`, ... env vars — §List values from env (L900)
  tests: `tests/env_var_list_test.rs::s13c_ev01_basic`, `s13c_ev02_stops_at_gap`
  status: ✅ — implemented in Phase 6 #3g (2026-05-18); lexer `'[' =>` arm in `parse_subst_body` + `resolve_env_list` helper
- **S13c.2** Stops at first missing index — §List values from env (L905)
  tests: `tests/env_var_list_test.rs::s13c_ev02_stops_at_gap`
  status: ✅
- **S13c.3** `${X[]}` no elements → required error — §List values from env (L910)
  tests: `tests/env_var_list_test.rs::s13c_ev03_required_no_elements_errors`
  status: ✅
- **S13c.4** `${?X[]}` no elements → undefined / removed — §List values from env (L912)
  tests: `tests/env_var_list_test.rs::s13c_ev04_optional_no_elements`
  status: ✅
- **S13c.5** `[]` suffix supported only for env vars (not config / sys props) — §List values from env (L902)
  tests: `tests/env_var_list_test.rs::s13c_s5_required_no_scalar_fallback`, `s13c_s5_optional_no_scalar_fallback`
  status: ✅ — scalar env fallback is suppressed when `list_suffix=true`; config-lookup returns early (E6)

## S14. Includes

### S14a. Include syntax

- **S14a.1** `include "filename"` (heuristic) — §Include syntax (L925)
  tests: src/parser.rs:767 (parses_include_directive); tests/include_test.rs:14 (parse_file_simple); tests/include_test.rs:21 (include_merges_into_current)
  status: ✅
- **S14a.2** `include url("...")` — §Include syntax (L927)
  out-of-scope: URL fetching is unsupported by design; declared as a Known Limitation in each implementation's README. HOCON.md L1175-1177 permits this: "Implementations need not support files, Java resources, or URLs."
  tests: tests/integration_test.rs:391 (test_include_url_not_supported)
  status: ➖
- **S14a.3** `include file("...")` — §Include syntax (L927)
  tests: src/parser.rs:779 (parses_include_file_syntax); tests/include_test.rs:109 (file_include_resolves_from_cwd_not_including_dir)
  status: ✅
- **S14a.4** `include classpath("...")` — §Include syntax (L927)
  out-of-scope: classpath resources are a JVM-only concept; non-JVM implementations have no equivalent loader.
  tests: tests/integration_test.rs:397 (test_include_classpath_not_supported)
  status: ➖
- **S14a.5** `include required(...)` — §Include syntax (L930)
  tests: tests/integration_test.rs:297 (test_include_required_file_form); tests/integration_test.rs:325 (test_include_required_missing_file_errors); tests/integration_test.rs:343 (test_include_required_existing_file_ok)
  status: ✅
- **S14a.6** Unquoted `include` at non-start-of-key is literal — §Include syntax (L962)
  tests: tests/integration_test.rs:1126 (s14a_6_include_in_dotted_key_is_literal)
  status: ✅
- **S14a.7** Whitespace allowed between `include` and resource name (incl. newlines) — §Include syntax (L952)
  tests: tests/spec_phase5.rs (s14a_7_whitespace_before_include_arg; s14a_7_newline_before_include_arg)
  status: ✅
- **S14a.8** No value concatenation on include argument — §Include syntax (L957)
  tests: tests/integration_test.rs:1137 (s14a_8_no_concatenation_on_include_arg)
  status: ✅
- **S14a.9** No substitutions in include argument — §Include syntax (L959)
  tests: tests/integration_test.rs:1146 (s14a_9_no_substitution_in_include_arg)
  status: ✅
- **S14a.10** Include argument must be quoted string — §Include syntax (L958)
  tests: tests/spec_phase5.rs (s14a_10_unquoted_include_arg_rejected)
  status: ✅
- **S14a.11** `"include"` (quoted) is just a normal key — §Include syntax (L977)
  tests: tests/spec_phase5.rs (s14a_11_quoted_include_is_normal_key)
  status: ✅

### S14b. Include semantics: merging

- **S14b.1** Included root must be an object (array → error) — §Include semantics: merging (L993)
  tests: tests/integration_test.rs:1155 (s14b_1_array_root_include_is_error)
  status: ✅
- **S14b.2** Included keys merge per duplicate-key rules — §Include semantics: merging (L997)
  tests: tests/include_test.rs:21 (include_merges_into_current)
  status: ✅
- **S14b.3** Earlier-in-including value + included → merged/overridden — §Include semantics: merging (L1000)
  tests: tests/include_test.rs:21 (include_merges_into_current)
  status: ✅
- **S14b.4** Later-in-including value overrides included — §Include semantics: merging (L1004)
  tests: tests/include_test.rs:29 (include_override_by_later_key)
  status: ✅

### S14c. Include semantics: substitution

- **S14c.1** Substitutions in included file are relativized to including scope — §Include semantics: substitution (L1019)
  tests: tests/include_test.rs:74 (include_relativize_quoted_key_with_dots); tests/integration_test.rs:510 (nested_include_resolves_substitutions_in_scope); tests/lightbend_test.rs:316 (lightbend_test10_nested_include)
  status: ✅
- **S14c.2** Original (non-relativized) path also tried as fallback — §Include semantics: substitution (L1048)
  tests: tests/lightbend_test.rs:221 (lightbend_test03_includes_with_substitution_fallback)
  status: ❌ ([#44](https://github.com/o3co/rs.hocon/issues/44)) — non-relativized fallback is not implemented; the cited test is written to accept either success or a substitution error, so passing the test does not prove correctness

### S14d. Include semantics: missing / required

- **S14d.1** Missing optional include silently ignored — §Include semantics: missing files (L1053)
  tests: tests/include_test.rs:64 (include_missing_silently_ignored); tests/integration_test.rs:361 (test_include_optional_missing_file_ok)
  status: ✅
- **S14d.2** Missing `required(...)` include → error — §Include semantics: missing files (L1057)
  tests: tests/integration_test.rs:325 (test_include_required_missing_file_errors); tests/integration_test.rs:334 (test_include_required_file_form_missing_errors)
  status: ✅
- **S14d.3** Non-missing IO errors NOT swallowed — §Include semantics: missing files (L1069)
  tests: tests/integration_test.rs:373 (test_include_probing_propagates_parse_error)
  status: ✅

### S14e. Include semantics: file formats & extensions

- **S14e.1** Extensionless basename probes multiple extensions — §Include semantics: file formats (L1080)
  tests: tests/include_test.rs:49 (include_extension_probing)
  status: ✅
- **S14e.2** Multiple matching extensions all loaded — §Include semantics: file formats (L1088)
  tests: tests/include_test.rs:56 (include_probe_order_conf_wins)
  status: ✅
- **S14e.3** Load order: `.properties` → `.json` → `.conf` — §Include semantics: file formats (L1091)
  tests: tests/include_test.rs:56 (include_probe_order_conf_wins)
  status: ✅
- **S14e.4** URL include: no extension probing (exact URL only) — §Include semantics: file formats (L1103)
  out-of-scope: URL include unsupported; see S14a.2.
  tests: —
  status: ➖
- **S14e.5** URL include: format from Content-Type or URL path extension — §Include semantics: file formats (L1104)
  out-of-scope: URL include unsupported; see S14a.2.
  tests: —
  status: ➖

### S14f. Include semantics: locating resources

- **S14f.1** Quoted-string heuristic: URL if valid protocol — §Include semantics: locating (L1115)
  out-of-scope: URL include unsupported; see S14a.2. The heuristic that distinguishes URL strings from filenames is moot when no URL form is supported.
  tests: —
  status: ➖
- **S14f.2** Otherwise treated as file/resource adjacent to including — §Include semantics: locating (L1117)
  tests: tests/include_test.rs:36 (include_nested_directory)
  status: ✅
- **S14f.3** Filesystem: relative path = relative to including dir (NOT cwd) — §Include semantics: locating (L1154)
  tests: tests/include_test.rs:109 (file_include_resolves_from_cwd_not_including_dir)
  status: ✅
- **S14f.4** Filesystem: absolute path preserved — §Include semantics: locating (L1152)
  tests: tests/integration_test.rs:297 (test_include_required_file_form)
  status: ✅
- **S14f.5** Filesystem: fall back to classpath on not-found — §Include semantics: locating (L1158)
  out-of-scope: classpath is JVM-only; see S14a.4.
  tests: —
  status: ➖
- **S14f.6** URL: "adjacent to" computed from URL path component — §Include semantics: locating (L1169)
  out-of-scope: URL include unsupported; see S14a.2.
  tests: —
  status: ➖
- **S14f.7** `url()`/`file()`/`classpath()` arguments NOT relativized — §Include semantics: locating (L1179)
  tests: tests/include_test.rs:109 (file_include_resolves_from_cwd_not_including_dir)
  status: ✅
- **S14f.8** `file:` URLs follow plain-filename filesystem semantics — §Include semantics: locating (L1171-1172)
  out-of-scope: URL include unsupported; see S14a.2. `file:` URLs are reachable only via `include url()`, which is not implemented.
  tests: —
  status: ➖

## S15. Numerically-indexed objects to arrays

- **S15.1** `{"0":"a","1":"b"}` → `["a","b"]` when array context — §Conversion (L1191)
  tests: tests/integration_test.rs (s15_1_num_indexed_obj_to_array_spec); tests/s15_fixtures.rs (na01_basic_get_list_returns_two_elements); src/numeric_array.rs (unit tests)
  status: ✅
  note: Implemented in fix/s15-numeric-obj-array (#79). Helper `numeric_object_to_array` in `src/numeric_array.rs` invoked from `Config::get_list` accessor site.
- **S15.2** Conversion is lazy (only on type-required access) — §Conversion (L1204)
  tests: tests/integration_test.rs (s15_2_conversion_is_lazy_spec); tests/s15_fixtures.rs (na02_lazy_get_config_still_works_and_get_list_also_works)
  status: ✅
  note: Laziness preserved: `get_config`/`get` do not invoke the helper. Conversion fires only from `get_list`. Verified by na02 fixture and spec test.
- **S15.3** Conversion in concatenation when list expected — §Conversion (L1210)
  tests: tests/integration_test.rs (s15_3_conversion_in_concatenation_spec); tests/s15_fixtures.rs (na03a, na03b, na03c, na03d)
  status: ✅
  note: Resolver pairwise-join in `src/resolver/substitution_resolver.rs::resolve_concat` calls `numeric_object_to_array` when an Object is concat'd with an Array. Multi-piece left-to-right pairwise ordering confirmed by na03d.
- **S15.4** Empty object NOT converted — §Conversion (L1212)
  tests: tests/integration_test.rs (s15_4_empty_object_not_converted); tests/s15_fixtures.rs (na04_empty_object_not_converted)
  status: ✅
  note: Empty-object guard is now explicit in `numeric_object_to_array` (returns None for empty maps). No longer incidental.
- **S15.5** Non-integer keys ignored during conversion — §Conversion (L1214)
  tests: tests/integration_test.rs (s15_5_non_integer_keys_ignored_spec); tests/s15_fixtures.rs (na05_non_int_keys_ignored)
  status: ✅
  note: Pre-filter regex `^(0|[1-9][0-9]*)$` rejects non-integer keys. Mixed-key objects produce only integer-keyed entries in output.
- **S15.6** Missing indices compacted in resulting array — §Conversion (L1216)
  tests: tests/integration_test.rs (s15_6_missing_indices_compacted_spec); tests/s15_fixtures.rs (na06_gaps_compacted)
  status: ✅
  note: Gaps eliminated naturally: eligible pairs are sorted and projected to value array, discarding index values.
- **S15.7** Sorted by integer key value — §Conversion (L1216)
  tests: tests/integration_test.rs (s15_7_sorted_by_key_value_spec); tests/s15_fixtures.rs (na07_sorted_by_key)
  status: ✅
  note: `sort_by_key` on parsed integer keys ensures deterministic order regardless of declaration order.

## S16. MIME Type

- **S16.1** Content-Type for HOCON resources is `application/hocon` — §MIME Type (L1223)
  out-of-scope: these implementations are parsers, not HTTP servers — they do not produce or advertise a Content-Type. The header is set by whoever serves a `.conf` file over HTTP.
  tests: —
  status: ➖

## S17. Automatic type conversions

- **S17.1** number → string (JSON-valid form) — §Automatic type conversions (L1235)
  tests: src/config.rs:536 (get_string_coerces_int); src/config.rs:542 (get_string_coerces_float); tests/integration_test.rs:237 (test_get_string_coerces_int); tests/integration_test.rs:243 (test_get_string_coerces_float)
  status: ✅
- **S17.2** boolean → string ("true" / "false") — §Automatic type conversions (L1237)
  tests: src/config.rs:551 (get_string_coerces_bool); tests/integration_test.rs:249 (test_get_string_coerces_bool)
  status: ✅
- **S17.3** string → number (JSON rules) — §Automatic type conversions (L1238)
  tests: src/config.rs:577 (get_i64_coerces_numeric_string); src/config.rs:609 (get_f64_coerces_numeric_string)
  status: ✅
- **S17.4** string → bool: `true`/`yes`/`on`/`false`/`no`/`off` — §Automatic type conversions (L1239)
  tests: src/config.rs:621 (get_bool_coerces_string_true); src/config.rs:627 (get_bool_coerces_string_false); src/config.rs:633 (get_bool_coerces_yes_no_on_off); tests/integration_test.rs:118 (parse_bool_coercion)
  status: ✅
- **S17.5** `"null"` → null when null requested — §Automatic type conversions (L1244)
  tests: tests/integration_test.rs:1449 (s17_5_null_string_stored_as_string_not_null)
  status: ➖
  note: rs.hocon has no `get_null()` typed getter — the "null requested" path does not exist in the API. Internally, quoted `"null"` is correctly stored as `ScalarType::String` (not `Null`), which is consistent with spec intent. Out-of-scope until a null-accessor is added.
- **S17.6** null → other type: error — §Automatic type conversions (L1252)
  tests: tests/integration_test.rs:1470 (s17_6_null_to_numeric_and_bool_errors); tests/integration_test.rs:1483 (s17_6_null_to_string_pin); tests/integration_test.rs:1495 (s17_6_null_to_string_spec)
  status: ⚠️
  note: `get_i64` and `get_bool` on null correctly error. `get_string` on null returns `Ok("null")` instead of an error — spec requires an error for all typed getters. Bug tracked in #80.
- **S17.7** object → other type: error — §Automatic type conversions (L1254)
  tests: src/config.rs:563 (get_string_error_on_object)
  status: ✅
- **S17.8** array → other (except numeric-indexed): error — §Automatic type conversions (L1255)
  tests: tests/integration_test.rs:1505 (s17_8_array_to_other_type_errors)
  status: ✅
  note: `get_string`, `get_i64`, `get_bool` all error on array values. `get_list` on a plain array succeeds.

## S18. Units format

- **S18.1** Number value taken as default unit — §Units format (L1279)
  tests: src/config.rs:813 (get_bytes_plain); src/config.rs:720 (get_duration_nanoseconds)
  status: ✅
- **S18.2** String parsed as: optional ws + number + ws + unit + ws — §Units format (L1281-1294)
  tests: src/config.rs:783 (get_duration_no_space); src/config.rs:867 (get_bytes_no_space)
  status: ✅
- **S18.3** Unit name letters-only (Unicode L* / `isLetter`) — §Units format (L1287)
  tests: tests/spec_phase5.rs (s18_3_unit_with_digit_rejected; s18_3_unit_with_hyphen_rejected; s18_3_valid_letter_only_unit_accepted)
  status: ✅
- **S18.4** String with no unit → interpreted with default unit — §Units format (L1290)
  tests: tests/spec_phase5.rs (s18_4_bytes_string_no_unit_uses_default; s18_4_spec_duration_string_no_unit_uses_default); tests/units_default_test.rs (ud01–ud08, up01–up05, ub01–ub06, un01–un03)
  status: ✅
  notes: All three families (duration→ms, period→days, bytes→bytes) now treat a bare number string as the default unit. `parse_duration` and `parse_bytes` use HOCON_WS trim and integer pre-classification. `parse_bytes` truncates fractional values per Lightbend `BigDecimal.toBigInteger()`. `get_bytes` rejects negative byte sizes at the accessor (positive-only invariant). `get_period`/`get_period_option` accessors added (net-new, return `(years: i32, months: i32, days: i32)`). rs-specific: `get_duration` returns `Err` for negative inputs (`std::time::Duration` is unsigned; see CHANGELOG).

## S19. Duration format

- **S19.1** `ns` / `nano` / `nanos` / `nanosecond` / `nanoseconds` — §Duration format (L1307)
  tests: src/config.rs:720 (get_duration_nanoseconds); tests/integration_test.rs:211 (test_duration_missing_units)
  status: ✅
- **S19.2** `us` / `micro` / `micros` / `microsecond` / `microseconds` — §Duration format (L1308)
  tests: tests/integration_test.rs:211 (test_duration_missing_units)
  status: ✅
- **S19.3** `ms` / `milli` / `millis` / `millisecond` / `milliseconds` — §Duration format (L1309)
  tests: src/config.rs:729 (get_duration_milliseconds); src/config.rs:783 (get_duration_no_space); tests/integration_test.rs:211 (test_duration_missing_units)
  status: ✅
- **S19.4** `s` / `second` / `seconds` — §Duration format (L1310)
  tests: src/config.rs:738 (get_duration_seconds); src/config.rs:792 (get_duration_singular_unit)
  status: ✅
- **S19.5** `m` / `minute` / `minutes` — §Duration format (L1311)
  tests: src/config.rs:747 (get_duration_minutes)
  status: ✅
- **S19.6** `h` / `hour` / `hours` — §Duration format (L1312)
  tests: src/config.rs:756 (get_duration_hours); src/config.rs:774 (get_duration_fractional)
  status: ✅
- **S19.7** `d` / `day` / `days` — §Duration format (L1313)
  tests: src/config.rs:765 (get_duration_days); tests/integration_test.rs:211 (test_duration_missing_units)
  status: ✅
- **S19.8** Duration unit names are case sensitive (lowercase only) — §Duration format (L1304)
  tests: tests/spec_phase5.rs (s19_8_pin_uppercase_ms_accepted; s19_8_spec_uppercase_ms_rejected [#ignore]; s19_8_pin_mixed_case_seconds_accepted; s19_8_spec_mixed_case_seconds_rejected [#ignore]; s19_8_lowercase_units_accepted)
  status: ❌
  notes: `parse_duration` calls `.to_lowercase()` on the unit string before matching, so `"MS"`, `"Seconds"`, `"NS"` etc. are wrongly accepted. Spec (L1304) requires lowercase only. Lowercase units still work correctly.

## S20. Period format

rs.hocon exposes `get_period` / `get_period_option` returning a `Period { years, months, days }`
struct. ts and go remain ➖ (no period accessor).

- **S20.1** `d` / `day` / `days` — §Period Format (L1327)
  tests: src/config.rs (parse_period_days_explicit, parse_period_bare_integer_uses_days_default); tests/units_default_test.rs (up01_period_bare, up02_period_leading_trailing_ws)
  status: ✅ (rs.hocon) / ➖ (ts, go)
- **S20.2** `w` / `week` / `weeks` — §Period Format (L1328)
  tests: src/config.rs (parse_period_weeks_unit); tests/units_default_test.rs (up05_period_with_unit)
  status: ✅ (rs.hocon) / ➖ (ts, go)
- **S20.3** `m` / `mo` / `month` / `months` — §Period Format (L1329)
  tests: src/config.rs (parse_period_months_unit)
  status: ✅ (rs.hocon) / ➖ (ts, go)
- **S20.4** `y` / `year` / `years` — §Period Format (L1333)
  tests: src/config.rs (parse_period_years_unit)
  status: ✅ (rs.hocon) / ➖ (ts, go)

## S21. Size in bytes format

- **S21.1** `B` / `b` / `byte` / `bytes` — §Size in bytes format (L1361)
  tests: src/config.rs:813 (get_bytes_plain)
  status: ✅
- **S21.2** Powers of 10 (kB, MB, GB, TB, PB, EB, ZB, YB + long forms) — §Size in bytes format (L1365)
  tests: src/config.rs:819 (get_bytes_kilobytes); src/config.rs:831 (get_bytes_megabytes); src/config.rs:843 (get_bytes_gigabytes); src/config.rs:855 (get_bytes_terabytes); src/config.rs:873 (get_bytes_long_unit)
  status: ✅
- **S21.3** Powers of 2 (K/Ki/KiB, M/Mi/MiB, ...) — §Size in bytes format (L1376)
  tests: src/config.rs:825 (get_bytes_kibibytes); src/config.rs:837 (get_bytes_mebibytes); src/config.rs:849 (get_bytes_gibibytes); src/config.rs:861 (get_bytes_tebibytes)
  status: ✅
- **S21.4** Single-letter abbreviations → powers of 2 (java -Xmx convention) — §Size in bytes format (L1385)
  tests: tests/spec_s21_4_single_letter_bytes.rs (s21_4_1_k_uppercase_is_1024 through s21_4_12_kib_stays_binary); tests/conformance_bsl.rs (bsl01-bsl09); tests/units_default_test.rs (ub05_bytes_with_unit — updated to 1_048_576)
  status: ✅ (was ✅ mis-classified; prior ✅ cited get_bytes_no_space which exercised '512MB' multi-letter, never single-letter K/M/G/T; behavior corrected — BREAKING: K/M/G/T now binary per L1385 — Phase 6 #3h)
- **S21.5** Fractional values supported (`0.5M`) — §Units format (L1281-1294) + §Size in bytes (L1335-1342)
  tests: tests/integration_test.rs:196 (test_parse_bytes_fractional); tests/integration_test.rs:203 (test_parse_bytes_fractional_binary); src/config.rs:891 (get_bytes_fractional_rounds)
  status: ✅

## S22. Config object merging API

- **S22.1** `merge(A, B)` semantics = duplicate-key behavior — §Config object merging (L1402)
  tests: src/config.rs:702 (with_fallback_receiver_wins); tests/integration_test.rs:135 (parse_with_fallback)
  status: ✅
- **S22.2** Intermediate non-object hides earlier object across files — §Config object merging (L1406)
  tests: tests/spec_phase5.rs (s22_2_non_object_hides_earlier_object_across_merge)
  status: ✅
- **S22.3** Setting key to null clears earlier object value — §Config object merging (L1436)
  tests: tests/spec_phase5.rs (s22_3_null_clears_earlier_object_in_merge)
  status: ✅

## S23. Java properties mapping

- **S23.1** Split key on `.` preserving empty strings — §Java properties (L1450)
  tests: src/properties.rs:91 (handles_dotted_keys)
  status: ✅
- **S23.2** Empty path elements (leading/trailing) preserved — §Java properties (L1456)
  tests: tests/spec_phase5.rs (s23_2_trailing_dot_creates_empty_key_segment; s23_2_leading_dot_creates_empty_key_segment)
  status: ✅
  notes: `"a."` creates path `["a",""]` (nested object with empty-string key inside `a`). `".a"` creates path `["","a"]` (empty-string root key containing `a`). The empty-string root key `""` is accessible via path `".a"` through `split_config_path`.
- **S23.3** Properties values are always strings — §Java properties (L1471)
  tests: src/properties.rs:109 (values_are_always_strings)
  status: ✅
- **S23.4** Object wins over string on conflicting key — §Java properties (L1485)
  tests: src/properties.rs (s23_4_forward_object_wins, s23_4_reverse_object_wins, s23_4_deep_forward_object_wins, s23_4_deep_reverse_object_wins); tests/conformance_properties_conflict.rs (pc01-pc04)
  status: ✅ (was ✅ mis-classified; prior ✅ cited converts_to_hocon_value which exercised 'a.b=1\nc=hello' — no conflict; set_nested had two bugs causing silent data-loss on conflict; corrected with object-wins rule + sorted-key processing — Phase 6 #3h)
- **S23.5** Multi-line values (backslash continuation) — §Note on Java properties similarity (L1587)
  out-of-scope: declared in each implementation's README — the `.properties` reader supports only basic `key=value` syntax to avoid pulling a full Java properties parser into a non-JVM library.
  tests: —
  status: ➖
- **S23.6** Unicode escapes in `.properties` — §Note on Java properties similarity (L1587)
  out-of-scope: same rationale as S23.5.
  tests: —
  status: ➖

## S24. Conventional config files (JVM)

- **S24.1** `reference.conf` classpath merge — §Conventional configuration files (L1502)
  out-of-scope: relies on classpath resource resolution (see S14a.4).
  tests: —
  status: ➖
- **S24.2** `application.{conf,json,properties}` default load — §Conventional configuration files (L1506)
  out-of-scope: relies on classpath resource resolution (see S14a.4).
  tests: —
  status: ➖

## S25. System property override

- **S25.1** System properties override config file values — §Conventional override (L1530)
  out-of-scope: JVM system properties are a JVM-only mechanism; non-JVM runtimes use environment variables or library-specific overrides.
  tests: —
  status: ➖

## S26. Substitution fallback to environment variables

- **S26.1** Env var lookup when substitution not in config tree — §Substitution fallback (L1536)
  tests: src/resolver/mod.rs:190 (resolves_env_var_fallback); tests/integration_test.rs:46 (parse_with_env_fallback); tests/include_test.rs:85 (include_env_fallback_quoted_key_prefix)
  status: ✅
- **S26.2** Empty env var preserved as empty string (not undefined) — §Substitution fallback (L1558)
  tests: tests/spec_phase5.rs (s26_2_empty_env_var_preserved_as_empty_string)
  status: ✅
- **S26.3** Env var SecurityException → treated as not present — §Substitution fallback (L1560)
  out-of-scope: `SecurityException` is a JVM-specific exception type; non-JVM runtimes have no equivalent guard at this layer.
  tests: —
  status: ➖
- **S26.4** Env vars always become strings (with auto type conversion) — §Substitution fallback (L1563)
  tests: src/resolver/mod.rs:172 (uses_env_var_when_present)
  status: ✅

---

## Extra-spec conventions (E-items)

Cross-implementation behaviors not enumerated by the Lightbend HOCON spec, tracked in
[`xx.hocon/docs/extra-spec-conventions.md`](https://github.com/o3co/xx.hocon/blob/main/docs/extra-spec-conventions.md).
rs.hocon status per-item:

- **E6** `${X[]}` config-defined wins — `[]` suffix is a no-op when X is a config key
  tests: `tests/env_var_list_test.rs::s13c_ev05_config_defined_wins`
  status: ✅ — config lookup returns early before `list_suffix` branch; env vars `X_0`, `X_1`, … are not consulted (Phase 6 #3g, 2026-05-18)
- **E7** ASCII SPACE (0x20) or TAB (0x09) between path expression and `[]` is allowed (`${X []}` / `${?X []}`); wider whitespace (NBSP, CR, Zs, Zl, BOM, …) is REJECTED to match the narrow extra-spec convention
  tests: `tests/env_var_list_test.rs::s13c_ev09_whitespace_before_suffix`, `tests/lexer_test.rs::lex_subst_list_suffix_e7_space`, `lex_subst_list_suffix_e7_tab`; negatives via `tests/env_var_list_test.rs::s13c_lex_nbsp_before_suffix_errors`, `_cr_before_suffix_errors`, `_zs_em_space_before_suffix_errors`
  status: ✅ — `parse_subst_body` `'['` arm validates `pending_ws` against `{' ', '\t'}` and rejects other HOCON whitespace (Phase 6 #3g, 2026-05-18; tightened in Copilot review fix on rs.hocon#88)
