//! Abstract syntax tree definitions for the toy language.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Str(String),
    Bool(bool),
    Nil,
    /// Variable reference. Carries the line it appeared on for error reporting.
    Identifier(String, usize),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        line: usize,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        line: usize,
    },
    Logical {
        op: LogicalOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Assign {
        name: String,
        value: Box<Expr>,
        line: usize,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        line: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        name: String,
        initializer: Expr,
        line: usize,
    },
    Expr(Expr),
    Block(Vec<Stmt>),
    If {
        cond: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    FunctionDecl {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
        line: usize,
    },
    Return {
        value: Option<Expr>,
        line: usize,
    },
}
