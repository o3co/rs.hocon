use crate::error::ResolveError;
use crate::lexer::Segment;
use crate::parser::{AstField, AstNode};
use crate::value::{HoconValue, ScalarValue};

use super::fold_self_ref::{contains_self_ref, fold_nested_self_refs, fold_or_skip_prior};
use super::include_loader::load_include;
#[cfg(feature = "include-package")]
use super::include_loader::load_package_include;
use super::types::{
    ConcatPlaceholder, InternalResolveOptions, ResObj, ResolverValue, SubstPlaceholder,
};
use super::utils::{deep_merge_res_obj_into, relativize_res_obj, string_segments_to_key};

pub(crate) struct StructureBuilder<'a> {
    opts: &'a InternalResolveOptions,
}

impl<'a> StructureBuilder<'a> {
    pub fn new(opts: &'a InternalResolveOptions) -> Self {
        StructureBuilder { opts }
    }

    pub fn build(&self, ast: AstNode, path_prefix: &[String]) -> Result<ResObj, ResolveError> {
        match ast {
            AstNode::Object { fields, .. } => {
                let mut obj = ResObj::new();
                for field in fields {
                    self.apply_field(&mut obj, field, path_prefix)?;
                }
                Ok(obj)
            }
            _ => Err(ResolveError {
                message: "root AST must be an object".into(),
                path: String::new(),
                line: 0,
                col: 0,
            }),
        }
    }

    fn apply_field(
        &self,
        obj: &mut ResObj,
        field: AstField,
        path_prefix: &[String],
    ) -> Result<(), ResolveError> {
        // Include directive
        if field.key.is_empty() {
            if let AstNode::Include {
                path: include_path,
                required,
                is_file,
                pos,
            } = &field.value
            {
                let mut included = load_include(
                    include_path,
                    *required,
                    *is_file,
                    pos.line,
                    pos.col,
                    self.opts,
                    path_prefix,
                )?;
                if !path_prefix.is_empty() {
                    relativize_res_obj(&mut included, path_prefix);
                }
                deep_merge_res_obj_into(obj, included, path_prefix);
                return Ok(());
            }

            // PackageInclude directive — E11
            #[cfg(feature = "include-package")]
            if let AstNode::PackageInclude {
                identifier,
                file,
                required,
                pos,
                ..
            } = &field.value
            {
                let mut included = load_package_include(
                    identifier, file, *required, pos.line, pos.col, self.opts,
                )?;
                if !path_prefix.is_empty() {
                    relativize_res_obj(&mut included, path_prefix);
                }
                deep_merge_res_obj_into(obj, included, path_prefix);
                return Ok(());
            }

            return Ok(());
        }

        let head = field.key[0].clone();
        let tail: Vec<String> = field.key[1..].to_vec();

        if !tail.is_empty() {
            // Nested key: create synthetic object
            let synthetic = AstNode::Object {
                fields: vec![AstField {
                    key: tail,
                    value: field.value,
                    append: field.append,
                    pos: field.pos.clone(),
                }],
                pos: field.pos.clone(),
            };
            return self.apply_field(
                obj,
                AstField {
                    key: vec![head],
                    value: synthetic,
                    append: false,
                    pos: field.pos,
                },
                path_prefix,
            );
        }

        if field.append {
            // S13b.2: `a += b` ≡ `a = ${?a} [b]` (HOCON.md L732). Desugar to that
            // exact concat AST and re-dispatch through the normal-assignment path
            // so every `+=` flows through the chained-self-ref machinery
            // (#118/#119/#120), which already accumulates `a = ${?a} [...]` as a
            // duplicate-key chain — including across include boundaries (the
            // cross-include splice in deep_merge_res_obj_into). The self-ref uses
            // the full nested path so `srv.items += x` references `${?srv.items}`,
            // and include relativization rewrites it under a mount prefix.
            //
            // Reset semantics (an explicit `a = [...]` before the `+=`) are
            // preserved because the explicit assignment records `a` in
            // `reset_keys`, which the merge uses to discard rather than splice the
            // destination's pre-merge value. See go.hocon#134.
            let mut child_prefix = path_prefix.to_vec();
            child_prefix.push(head.clone());
            let segments: Vec<Segment> = child_prefix
                .iter()
                .map(|s| Segment {
                    text: s.clone(),
                    line: field.pos.line,
                    col: field.pos.col,
                })
                .collect();
            let subst = AstNode::Substitution {
                segments,
                optional: true,
                list_suffix: false,
                pos: field.pos.clone(),
            };
            let elem_array = AstNode::Array {
                items: vec![field.value],
                pos: field.pos.clone(),
            };
            let synthetic = AstNode::Concat {
                nodes: vec![subst, elem_array],
                pos: field.pos.clone(),
            };
            return self.apply_field(
                obj,
                AstField {
                    key: vec![head],
                    value: synthetic,
                    append: false,
                    pos: field.pos,
                },
                path_prefix,
            );
        }

        // Normal assignment
        let existing = obj.fields.get(&head).cloned();
        let mut child_prefix = path_prefix.to_vec();
        child_prefix.push(head.clone());
        let full_key = string_segments_to_key(child_prefix.iter().map(String::as_str));
        let new_val = self.ast_to_resolver_value(field.value, &child_prefix)?;

        // go.hocon#134: a non-self-referential assignment to `head` is a *reset* —
        // its net value does not chain off an outer `${?head}`. Record it so a
        // cross-include merge discards (rather than splices onto) the
        // destination's pre-merge value. A desugared `+=` (or an explicit
        // `head = ${?head} ...`) is self-referential, so it is NOT a reset and the
        // chain continues across the include boundary. Once set, the flag stays
        // set: a later `+=` chains off the reset value, so the net contribution
        // still does not chain off an outer prior.
        if !contains_self_ref(&new_val, &full_key) {
            obj.reset_keys.insert(head.clone());
        }

        // #118 + #120: save existing with fold-or-skip (chain-class fix).
        //
        // xx.hocon#27 cluster 3h sr13: save the previous value before any
        // nested fold. If this stores a post-fold object, the next overwrite
        // can fold that already-expanded value again (`xbarbar`).
        if let Some(ref ex) = existing {
            let old_prior = obj.prior_values.get(&head).cloned();
            let prior_source;
            // xx.hocon#27 review #124 Issue 3 (cross-impl with ts.hocon Codex P1 + Claude #1):
            // fold nested self-refs in `existing` when `existing` is an Obj and either:
            //   (a) `new_val` is NOT an Obj (e.g. `o = ${?o}` overwriting `{a:Concat[...]}`)
            //       — prior must capture pre-overwrite state with sub-fields resolved so
            //       `${?o}` at resolve time returns `{a:"xbar"}` not `{a:"bar"}`; or
            //   (b) `new_val` IS an Obj whose keys are a subset of `existing`'s keys
            //       (same-key Obj-Obj overwrite, e.g. `o = {a=1, prev=${?o}}` after
            //       `o.a = "xbar"`) — same sub-field resolution requirement applies.
            //
            // Do NOT fold for the sr13 case: Obj-Obj adding new keys.  When `new_val`
            // has keys not in `existing`, the leaf-prior fallback in resolve_subst_inner
            // handles sub-field resolution; folding here would double-fold on the 3rd
            // write producing the "xbarbar" regression.
            let should_fold_nested = match (ex, &new_val) {
                (ResolverValue::Obj(_), ResolverValue::Obj(new_obj)) => {
                    if let ResolverValue::Obj(existing_obj) = ex {
                        // Fold only when new keys are a subset of existing keys (same-key
                        // or strict-subset Obj-Obj).  Adding new keys → skip (sr13 guard).
                        new_obj
                            .fields
                            .keys()
                            .all(|k| existing_obj.fields.contains_key(k))
                    } else {
                        false
                    }
                }
                (ResolverValue::Obj(_), _) => true, // Obj overwritten by non-Obj
                _ => false,
            };
            let prior_input = if should_fold_nested {
                prior_source = fold_nested_self_refs(ex, &child_prefix);
                &prior_source
            } else {
                ex
            };
            if let Some(prior) = fold_or_skip_prior(prior_input, &full_key, old_prior.as_ref()) {
                obj.prior_values.insert(head.clone(), prior);
            }
        }

        // Deep merge if both are ResObj
        if let (Some(ResolverValue::Obj(_)), ResolverValue::Obj(new_obj)) = (&existing, &new_val) {
            if let Some(ResolverValue::Obj(existing_obj)) = obj.fields.get_mut(&head) {
                deep_merge_res_obj_into(existing_obj, new_obj.clone(), &child_prefix);
                return Ok(());
            }
        }

        obj.fields.insert(head, new_val);
        Ok(())
    }

    fn ast_to_resolver_value(
        &self,
        ast: AstNode,
        path_prefix: &[String],
    ) -> Result<ResolverValue, ResolveError> {
        match ast {
            AstNode::Scalar { value, .. } => Ok(ResolverValue::Resolved(HoconValue::Scalar(value))),
            AstNode::Array { items, .. } => {
                let rv_items: Vec<ResolverValue> = items
                    .into_iter()
                    .map(|item| self.ast_to_resolver_value(item, path_prefix))
                    .collect::<Result<_, _>>()?;
                let all_resolved = rv_items
                    .iter()
                    .all(|v| matches!(v, ResolverValue::Resolved(_)));
                if all_resolved {
                    let hv_items: Vec<HoconValue> = rv_items
                        .into_iter()
                        .map(|v| match v {
                            ResolverValue::Resolved(hv) => hv,
                            _ => unreachable!(),
                        })
                        .collect();
                    Ok(ResolverValue::Resolved(HoconValue::Array(hv_items)))
                } else {
                    Ok(ResolverValue::UnresolvedArray(rv_items))
                }
            }
            AstNode::Object { .. } => {
                let inner = self.build(ast, path_prefix)?;
                Ok(ResolverValue::Obj(inner))
            }
            AstNode::Substitution {
                segments,
                optional,
                list_suffix,
                pos,
                ..
            } => Ok(ResolverValue::Subst(SubstPlaceholder {
                segments,
                optional,
                known_absent: false,
                list_suffix,
                line: pos.line,
                col: pos.col,
                prefix_len: 0,
            })),
            AstNode::Concat { nodes, pos } => {
                let mut separator_flags = Vec::with_capacity(nodes.len());
                let mut rv_nodes = Vec::with_capacity(nodes.len());
                for node in nodes {
                    let is_sep = matches!(
                        node,
                        AstNode::Scalar {
                            separator: true,
                            ..
                        }
                    );
                    rv_nodes.push(self.ast_to_resolver_value(node, path_prefix)?);
                    separator_flags.push(is_sep);
                }
                Ok(ResolverValue::Concat(ConcatPlaceholder {
                    nodes: rv_nodes,
                    separator_flags,
                    line: pos.line,
                    col: pos.col,
                }))
            }
            AstNode::Include { .. } => Ok(ResolverValue::Resolved(HoconValue::Scalar(
                ScalarValue::null(),
            ))),
            // PackageInclude in value position is unreachable: the parser only
            // emits PackageInclude as an include directive (key=[]), not as a value.
            #[cfg(feature = "include-package")]
            AstNode::PackageInclude { .. } => Ok(ResolverValue::Resolved(HoconValue::Scalar(
                ScalarValue::null(),
            ))),
        }
    }
}
