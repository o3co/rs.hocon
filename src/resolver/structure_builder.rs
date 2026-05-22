use crate::error::ResolveError;
use crate::parser::{AstField, AstNode};
use crate::value::{HoconValue, ScalarValue};

use super::fold_self_ref::{fold_nested_self_refs, fold_or_skip_prior};
use super::include_loader::load_include;
#[cfg(feature = "include-package")]
use super::include_loader::load_package_include;
use super::types::{
    AppendPlaceholder, ConcatPlaceholder, InternalResolveOptions, ResObj, ResolverValue,
    SubstPlaceholder,
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
            let existing = obj
                .fields
                .get(&head)
                .cloned()
                .unwrap_or_else(|| ResolverValue::Resolved(HoconValue::Array(vec![])));
            let mut child_prefix = path_prefix.to_vec();
            child_prefix.push(head.clone());
            // #118 + #120 cross-impl with go.hocon: fold self-references in
            // `existing` against the OLD prior so the recorded prior is
            // self-ref-free. Chain invariant: by induction every saved prior
            // contains no `${full_key}` reference.
            let full_key = string_segments_to_key(child_prefix.iter().map(String::as_str));
            let old_prior = obj.prior_values.get(&head).cloned();
            if let Some(prior) = fold_or_skip_prior(&existing, &full_key, old_prior.as_ref()) {
                obj.prior_values.insert(head.clone(), prior);
            }
            let elem = self.ast_to_resolver_value(field.value, &child_prefix)?;
            let field_line = field.pos.line;
            let field_col = field.pos.col;
            obj.fields.insert(
                head,
                ResolverValue::Append(AppendPlaceholder {
                    existing: Box::new(existing),
                    elem: Box::new(elem),
                    line: field_line,
                    col: field_col,
                }),
            );
            return Ok(());
        }

        // Normal assignment
        let existing = obj.fields.get(&head).cloned();
        let mut child_prefix = path_prefix.to_vec();
        child_prefix.push(head.clone());
        let new_val = self.ast_to_resolver_value(field.value, &child_prefix)?;

        // #118 + #120: save existing with fold-or-skip (chain-class fix).
        // Cross-impl with go.hocon PR #121 / #123. Applies regardless of
        // whether new_val will deep-merge with existing — the previous
        // unconditional save still worked for #118-chain crashes because
        // ResolverValue::Obj couldn't contain a self-ref placeholder visible
        // to the prior-save layer (only the merged sub-object's interior
        // could, see #120). With fold + walker extension covering Obj /
        // UnresolvedArray interiors, the saved prior is self-ref-free.
        //
        // fold_nested_self_refs pre-pass handles the multi-segment chain
        // (`r.x = ${r.x} [...]` × N): rs.hocon's resolve_subst navigates
        // prior_root.fields per segment, so a leaf-level concat containing
        // ${r.x} retained in the nested ResObj would loop during prior
        // resolution. The pre-pass folds those nested ${X.Y} refs against
        // each enclosing ResObj's prior_values before the outer save.
        if let Some(ref ex) = existing {
            let full_key = string_segments_to_key(child_prefix.iter().map(String::as_str));
            let ex_folded = fold_nested_self_refs(ex, &child_prefix);
            let old_prior = obj.prior_values.get(&head).cloned();
            if let Some(prior) = fold_or_skip_prior(&ex_folded, &full_key, old_prior.as_ref()) {
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
