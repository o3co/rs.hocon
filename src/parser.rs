use crate::error::ParseError;
use crate::lexer::Token;
use crate::value::ScalarValue;

#[derive(Debug, Clone)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub enum AstNode {
    Object { fields: Vec<AstField>, pos: Pos },
    Array { items: Vec<AstNode>, pos: Pos },
    Scalar { value: ScalarValue, pos: Pos },
    Concat { nodes: Vec<AstNode>, pos: Pos },
    Substitution { path: String, optional: bool, pos: Pos },
    Include { path: String, pos: Pos },
}

#[derive(Debug, Clone)]
pub struct AstField {
    pub key: Vec<String>,
    pub value: AstNode,
    pub append: bool,
    pub pos: Pos,
}

pub fn parse_tokens(_tokens: &[Token]) -> Result<AstNode, ParseError> {
    Ok(AstNode::Object { fields: vec![], pos: Pos { line: 1, col: 1 } })
}
