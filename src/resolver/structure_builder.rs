use crate::error::ResolveError;
use crate::parser::{AstField, AstNode};
use crate::value::{HoconValue, ScalarValue};

use super::include_loader::load_include;
use super::types::{
    AppendPlaceholder, ConcatPlaceholder, ResObj, ResolveOptions, ResolverValue, SubstPlaceholder,
};
use super::utils::{deep_merge_res_obj_into, relativize_res_obj};

pub(crate) struct StructureBuilder<'a> {
    opts: &'a ResolveOptions,
}

impl<'a> StructureBuilder<'a> {
    pub fn new(opts: &'a ResolveOptions) -> Self {
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
                pos,
            } = &field.value
            {
                let mut included = load_include(
                    include_path,
                    *required,
                    pos.line,
                    pos.col,
                    self.opts,
                    path_prefix,
                )?;
                if !path_prefix.is_empty() {
                    let prefix_str = path_prefix.join(".");
                    relativize_res_obj(&mut included, &prefix_str, path_prefix.len());
                }
                deep_merge_res_obj_into(obj, included);
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
            obj.prior_values.insert(head.clone(), existing.clone());
            let mut child_prefix = path_prefix.to_vec();
            child_prefix.push(head.clone());
            let elem = self.ast_to_resolver_value(field.value, &child_prefix)?;
            obj.fields.insert(
                head,
                ResolverValue::Append(AppendPlaceholder {
                    existing: Box::new(existing),
                    elem: Box::new(elem),
                }),
            );
            return Ok(());
        }

        // Normal assignment
        let existing = obj.fields.get(&head).cloned();
        let mut child_prefix = path_prefix.to_vec();
        child_prefix.push(head.clone());
        let new_val = self.ast_to_resolver_value(field.value, &child_prefix)?;

        if let Some(ref ex) = existing {
            obj.prior_values.insert(head.clone(), ex.clone());
        }

        // Deep merge if both are ResObj
        if let (Some(ResolverValue::Obj(_)), ResolverValue::Obj(new_obj)) = (&existing, &new_val) {
            if let Some(ResolverValue::Obj(existing_obj)) = obj.fields.get_mut(&head) {
                deep_merge_res_obj_into(existing_obj, new_obj.clone());
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
                path,
                optional,
                pos,
            } => Ok(ResolverValue::Subst(SubstPlaceholder {
                path,
                optional,
                line: pos.line,
                col: pos.col,
                prefix_len: 0,
            })),
            AstNode::Concat { nodes, .. } => {
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
                }))
            }
            AstNode::Include { .. } => Ok(ResolverValue::Resolved(HoconValue::Scalar(
                ScalarValue::Null,
            ))),
        }
    }
}
