use crate::value::HoconValue;
use indexmap::IndexMap;

use super::types::{ResObj, ResolverValue};

pub(crate) fn parse_subst_path(raw: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }

        if chars[i] == '"' {
            i += 1;
            let mut seg = String::new();
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\'
                    && i + 1 < chars.len()
                    && (chars[i + 1] == '"' || chars[i + 1] == '\\')
                {
                    seg.push(chars[i + 1]);
                    i += 2;
                } else {
                    seg.push(chars[i]);
                    i += 1;
                }
            }
            if i < chars.len() {
                i += 1;
            }
            segments.push(seg);
            while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
                i += 1;
            }
            if i < chars.len() && chars[i] == '.' {
                i += 1;
            }
        } else if chars[i] == '.' {
            segments.push(String::new());
            i += 1;
        } else {
            let mut seg = String::new();
            while i < chars.len() && chars[i] != '.' {
                seg.push(chars[i]);
                i += 1;
            }
            segments.push(seg.trim().to_string());
            if i < chars.len() && chars[i] == '.' {
                i += 1;
            }
        }
    }

    segments
}

pub(crate) fn lookup_path<'a>(root: &'a ResObj, segments: &[String]) -> Option<&'a ResolverValue> {
    if segments.is_empty() {
        return None;
    }
    let head = &segments[0];
    let tail = &segments[1..];
    let val = root.fields.get(head.as_str())?;
    if tail.is_empty() {
        return Some(val);
    }
    if let ResolverValue::Obj(inner) = val {
        return lookup_path(inner, tail);
    }
    None
}

pub(crate) fn deep_merge_hocon_objects(
    base: IndexMap<String, HoconValue>,
    overlay: IndexMap<String, HoconValue>,
) -> HoconValue {
    let mut merged = base;
    for (k, v) in overlay {
        if let (Some(HoconValue::Object(existing)), HoconValue::Object(new_fields)) =
            (merged.get(&k).cloned(), &v)
        {
            merged.insert(k, deep_merge_hocon_objects(existing, new_fields.clone()));
        } else {
            merged.insert(k, v);
        }
    }
    HoconValue::Object(merged)
}

/// Convert a Resolved(Object) into a ResObj so we can deep-merge it.
fn resolved_obj_to_res_obj(fields: &IndexMap<String, HoconValue>) -> ResObj {
    let mut obj = ResObj::new();
    for (k, v) in fields {
        obj.fields
            .insert(k.clone(), ResolverValue::Resolved(v.clone()));
    }
    obj
}

/// Extract the inner ResObj if the value is an object-like ResolverValue.
/// Returns Some(ResObj) for Obj or Resolved(Object), None otherwise.
fn as_res_obj(val: &ResolverValue) -> Option<ResObj> {
    match val {
        ResolverValue::Obj(o) => Some(o.clone()),
        ResolverValue::Resolved(HoconValue::Object(fields)) => {
            Some(resolved_obj_to_res_obj(fields))
        }
        _ => None,
    }
}

pub(crate) fn deep_merge_res_obj_into(dst: &mut ResObj, src: ResObj) {
    for (k, src_val) in src.fields {
        let dst_is_obj = dst.fields.get(&k).and_then(as_res_obj);
        let src_obj = as_res_obj(&src_val);

        if let (Some(mut dst_obj), Some(src_obj)) = (dst_is_obj, src_obj) {
            deep_merge_res_obj_into(&mut dst_obj, src_obj);
            dst.fields.insert(k, ResolverValue::Obj(dst_obj));
            continue;
        }

        if let Some(old) = dst.fields.get(&k) {
            dst.prior_values.insert(k.clone(), old.clone());
        }
        dst.fields.insert(k, src_val);
    }
    // Carry over prior_values from src that aren't already set in dst.
    // This preserves delayed-merge chains from included files.
    for (k, src_prior) in src.prior_values {
        if !dst.prior_values.contains_key(&k) {
            dst.prior_values.insert(k, src_prior);
        }
    }
}

/// Relativize all substitution paths in a ResolverValue tree by prepending the given prefix.
/// Called when including a file into a nested scope so `${y}` becomes `${prefix.y}`.
pub(crate) fn relativize_subst_paths(val: &mut ResolverValue, prefix_segments: &[String]) {
    match val {
        ResolverValue::Subst(s) => {
            let mut new_segments = Vec::with_capacity(prefix_segments.len() + s.segments.len());
            new_segments.extend_from_slice(prefix_segments);
            new_segments.extend_from_slice(&s.segments);
            s.segments = new_segments;
            s.prefix_len += prefix_segments.len();
        }
        ResolverValue::Concat(c) => {
            for node in &mut c.nodes {
                relativize_subst_paths(node, prefix_segments);
            }
        }
        ResolverValue::Append(a) => {
            relativize_subst_paths(&mut a.existing, prefix_segments);
            relativize_subst_paths(&mut a.elem, prefix_segments);
        }
        ResolverValue::Obj(o) => {
            relativize_res_obj(o, prefix_segments);
        }
        ResolverValue::UnresolvedArray(items) => {
            for item in items {
                relativize_subst_paths(item, prefix_segments);
            }
        }
        ResolverValue::Resolved(_) => {}
    }
}

pub(crate) fn relativize_res_obj(obj: &mut ResObj, prefix_segments: &[String]) {
    for val in obj.fields.values_mut() {
        relativize_subst_paths(val, prefix_segments);
    }
    for val in obj.prior_values.values_mut() {
        relativize_subst_paths(val, prefix_segments);
    }
}

pub(crate) fn segments_to_key(segments: &[String]) -> String {
    segments
        .iter()
        .map(|s| {
            if s.is_empty()
                || s.contains('.')
                || s.contains('"')
                || s.contains('\\')
                || s != s.trim()
                || s.contains(' ')
                || s.contains('\t')
            {
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{}\"", escaped)
            } else {
                s.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_to_key_simple() {
        assert_eq!(
            segments_to_key(&["a".into(), "b".into(), "c".into()]),
            "a.b.c"
        );
    }

    #[test]
    fn segments_to_key_quoted_dot() {
        assert_eq!(segments_to_key(&["a.b".into(), "c".into()]), r#""a.b".c"#);
    }

    #[test]
    fn segments_to_key_empty_string() {
        assert_eq!(segments_to_key(&["".into(), "foo".into()]), r#""".foo"#);
    }

    #[test]
    fn segments_to_key_escaped_quotes() {
        assert_eq!(segments_to_key(&["a\"b".into(), "c".into()]), r#""a\"b".c"#);
    }

    #[test]
    fn segments_to_key_escaped_backslash() {
        assert_eq!(segments_to_key(&["a\\b".into(), "c".into()]), r#""a\\b".c"#);
    }

    #[test]
    fn segments_to_key_roundtrip_with_special_chars() {
        let cases: Vec<Vec<String>> = vec![
            vec!["a\"b".into(), "c".into()],
            vec!["a\\b".into(), "c".into()],
        ];
        for segs in &cases {
            let key = segments_to_key(segs);
            let parsed = parse_subst_path(&key);
            assert_eq!(
                &parsed, segs,
                "roundtrip failed for {:?} → {:?} → {:?}",
                segs, key, parsed
            );
        }
    }

    #[test]
    fn parse_subst_path_preserves_unknown_escapes() {
        // \n inside quotes should be kept as literal \n, not stripped to n
        assert_eq!(parse_subst_path(r#""a\nb""#), vec!["a\\nb".to_string()]);
    }

    #[test]
    fn segments_to_key_quotes_whitespace() {
        assert_eq!(segments_to_key(&[" a ".into(), "b".into()]), r#"" a ".b"#);
        // roundtrip
        let segs = vec![" a ".into(), "b".into()];
        let key = segments_to_key(&segs);
        assert_eq!(parse_subst_path(&key), segs);
    }

    #[test]
    fn segments_to_key_roundtrip() {
        let cases: Vec<Vec<String>> = vec![
            vec!["a".into(), "b".into()],
            vec!["a.b".into(), "c".into()],
            vec!["".into(), "x".into(), "".into()],
            vec!["a.b.c".into(), "d.e".into()],
        ];
        for segs in &cases {
            let key = segments_to_key(segs);
            let parsed = parse_subst_path(&key);
            assert_eq!(
                &parsed, segs,
                "roundtrip failed for {:?} → {:?} → {:?}",
                segs, key, parsed
            );
        }
    }
}
