//! Recursive-descent parser: turns a token stream into an AST.
//!
//! Precedence (low to high):
//!   assignment -> logic_or -> logic_and -> equality -> comparison
//!   -> term (+ -) -> factor (* /) -> unary (! -) -> call -> primary

use crate::ast::{BinaryOp, Expr, LogicalOp, Stmt, UnaryOp};
use crate::lexer::{Token, TokenKind};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Parse error at line {}: {}", self.line, self.message)
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

type PResult<T> = Result<T, ParseError>;

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    pub fn parse_program(mut self) -> PResult<Vec<Stmt>> {
        let mut stmts = Vec::new();
        while !self.is_at_end() {
            stmts.push(self.declaration()?);
        }
        Ok(stmts)
    }

    // ---- statements ----

    fn declaration(&mut self) -> PResult<Stmt> {
        if self.match_kind(&TokenKind::Fn) {
            return self.function_decl();
        }
        if self.match_kind(&TokenKind::Let) {
            return self.let_decl();
        }
        self.statement()
    }

    fn function_decl(&mut self) -> PResult<Stmt> {
        let line = self.previous().line;
        let name = self.consume_identifier("expected function name after 'fn'")?;
        self.consume(&TokenKind::LParen, "expected '(' after function name")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                params.push(self.consume_identifier("expected parameter name")?);
                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.consume(&TokenKind::RParen, "expected ')' after parameters")?;
        self.consume(&TokenKind::LBrace, "expected '{' before function body")?;
        let body = self.block()?;
        Ok(Stmt::FunctionDecl {
            name,
            params,
            body,
            line,
        })
    }

    fn let_decl(&mut self) -> PResult<Stmt> {
        let line = self.previous().line;
        let name = self.consume_identifier("expected variable name after 'let'")?;
        self.consume(&TokenKind::Eq, "expected '=' after variable name")?;
        let initializer = self.expression()?;
        self.consume(&TokenKind::Semicolon, "expected ';' after variable declaration")?;
        Ok(Stmt::Let {
            name,
            initializer,
            line,
        })
    }

    fn statement(&mut self) -> PResult<Stmt> {
        if self.match_kind(&TokenKind::If) {
            return self.if_stmt();
        }
        if self.match_kind(&TokenKind::While) {
            return self.while_stmt();
        }
        if self.match_kind(&TokenKind::Return) {
            return self.return_stmt();
        }
        if self.match_kind(&TokenKind::LBrace) {
            return Ok(Stmt::Block(self.block()?));
        }
        self.expr_stmt()
    }

    fn if_stmt(&mut self) -> PResult<Stmt> {
        self.consume(&TokenKind::LParen, "expected '(' after 'if'")?;
        let cond = self.expression()?;
        self.consume(&TokenKind::RParen, "expected ')' after if condition")?;
        self.consume(&TokenKind::LBrace, "expected '{' to start if body")?;
        let then_branch = self.block()?;
        let else_branch = if self.match_kind(&TokenKind::Else) {
            if self.match_kind(&TokenKind::If) {
                Some(vec![self.if_stmt()?])
            } else {
                self.consume(&TokenKind::LBrace, "expected '{' to start else body")?;
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(Stmt::If {
            cond,
            then_branch,
            else_branch,
        })
    }

    fn while_stmt(&mut self) -> PResult<Stmt> {
        self.consume(&TokenKind::LParen, "expected '(' after 'while'")?;
        let cond = self.expression()?;
        self.consume(&TokenKind::RParen, "expected ')' after while condition")?;
        self.consume(&TokenKind::LBrace, "expected '{' to start while body")?;
        let body = self.block()?;
        Ok(Stmt::While { cond, body })
    }

    fn return_stmt(&mut self) -> PResult<Stmt> {
        let line = self.previous().line;
        let value = if self.check(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.expression()?)
        };
        self.consume(&TokenKind::Semicolon, "expected ';' after return value")?;
        Ok(Stmt::Return { value, line })
    }

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            stmts.push(self.declaration()?);
        }
        self.consume(&TokenKind::RBrace, "expected '}' after block")?;
        Ok(stmts)
    }

    fn expr_stmt(&mut self) -> PResult<Stmt> {
        let expr = self.expression()?;
        self.consume(&TokenKind::Semicolon, "expected ';' after expression")?;
        Ok(Stmt::Expr(expr))
    }

    // ---- expressions ----

    fn expression(&mut self) -> PResult<Expr> {
        self.assignment()
    }

    fn assignment(&mut self) -> PResult<Expr> {
        let expr = self.logic_or()?;

        if self.match_kind(&TokenKind::Eq) {
            let eq_line = self.previous().line;
            let value = self.assignment()?;
            if let Expr::Identifier(name, _) = expr {
                return Ok(Expr::Assign {
                    name,
                    value: Box::new(value),
                    line: eq_line,
                });
            }
            return Err(ParseError {
                message: "invalid assignment target".to_string(),
                line: eq_line,
            });
        }
        Ok(expr)
    }

    fn logic_or(&mut self) -> PResult<Expr> {
        let mut expr = self.logic_and()?;
        while self.match_kind(&TokenKind::OrOr) {
            let right = self.logic_and()?;
            expr = Expr::Logical {
                op: LogicalOp::Or,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn logic_and(&mut self) -> PResult<Expr> {
        let mut expr = self.equality()?;
        while self.match_kind(&TokenKind::AndAnd) {
            let right = self.equality()?;
            expr = Expr::Logical {
                op: LogicalOp::And,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn equality(&mut self) -> PResult<Expr> {
        let mut expr = self.comparison()?;
        loop {
            let op = if self.match_kind(&TokenKind::EqEq) {
                BinaryOp::Eq
            } else if self.match_kind(&TokenKind::NotEq) {
                BinaryOp::NotEq
            } else {
                break;
            };
            let line = self.previous().line;
            let right = self.comparison()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn comparison(&mut self) -> PResult<Expr> {
        let mut expr = self.term()?;
        loop {
            let op = if self.match_kind(&TokenKind::Lt) {
                BinaryOp::Lt
            } else if self.match_kind(&TokenKind::LtEq) {
                BinaryOp::LtEq
            } else if self.match_kind(&TokenKind::Gt) {
                BinaryOp::Gt
            } else if self.match_kind(&TokenKind::GtEq) {
                BinaryOp::GtEq
            } else {
                break;
            };
            let line = self.previous().line;
            let right = self.term()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn term(&mut self) -> PResult<Expr> {
        let mut expr = self.factor()?;
        loop {
            let op = if self.match_kind(&TokenKind::Plus) {
                BinaryOp::Add
            } else if self.match_kind(&TokenKind::Minus) {
                BinaryOp::Sub
            } else {
                break;
            };
            let line = self.previous().line;
            let right = self.factor()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn factor(&mut self) -> PResult<Expr> {
        let mut expr = self.unary()?;
        loop {
            let op = if self.match_kind(&TokenKind::Star) {
                BinaryOp::Mul
            } else if self.match_kind(&TokenKind::Slash) {
                BinaryOp::Div
            } else {
                break;
            };
            let line = self.previous().line;
            let right = self.unary()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
                line,
            };
        }
        Ok(expr)
    }

    fn unary(&mut self) -> PResult<Expr> {
        if self.match_kind(&TokenKind::Bang) {
            let line = self.previous().line;
            let expr = self.unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
                line,
            });
        }
        if self.match_kind(&TokenKind::Minus) {
            let line = self.previous().line;
            let expr = self.unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
                line,
            });
        }
        self.call()
    }

    fn call(&mut self) -> PResult<Expr> {
        let mut expr = self.primary()?;
        loop {
            if self.match_kind(&TokenKind::LParen) {
                let line = self.previous().line;
                let mut args = Vec::new();
                if !self.check(&TokenKind::RParen) {
                    loop {
                        args.push(self.expression()?);
                        if !self.match_kind(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.consume(&TokenKind::RParen, "expected ')' after arguments")?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    line,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn primary(&mut self) -> PResult<Expr> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::Number(n) => {
                self.advance();
                Ok(Expr::Number(n))
            }
            TokenKind::Str(s) => {
                self.advance();
                Ok(Expr::Str(s))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            TokenKind::Nil => {
                self.advance();
                Ok(Expr::Nil)
            }
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(Expr::Identifier(name, tok.line))
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.expression()?;
                self.consume(&TokenKind::RParen, "expected ')' after expression")?;
                Ok(expr)
            }
            _ => Err(ParseError {
                message: format!("unexpected token {:?}", tok.kind),
                line: tok.line,
            }),
        }
    }

    // ---- helpers ----

    fn consume_identifier(&mut self, msg: &str) -> PResult<String> {
        if let TokenKind::Identifier(name) = self.peek().kind.clone() {
            self.advance();
            Ok(name)
        } else {
            Err(ParseError {
                message: msg.to_string(),
                line: self.peek().line,
            })
        }
    }

    fn consume(&mut self, kind: &TokenKind, msg: &str) -> PResult<&Token> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(ParseError {
                message: format!("{} (found {:?})", msg, self.peek().kind),
                line: self.peek().line,
            })
        }
    }

    fn match_kind(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind)
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.pos += 1;
        }
        self.previous()
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.pos - 1]
    }
}

pub fn parse(tokens: Vec<Token>) -> PResult<Vec<Stmt>> {
    Parser::new(tokens).parse_program()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse_src(src: &str) -> Vec<Stmt> {
        parse(tokenize(src).unwrap()).unwrap()
    }

    #[test]
    fn parses_let_with_precedence() {
        // 1 + 2 * 3 should parse as 1 + (2 * 3)
        let stmts = parse_src("let x = 1 + 2 * 3;");
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Let { name, initializer, .. } => {
                assert_eq!(name, "x");
                match initializer {
                    Expr::Binary { op: BinaryOp::Add, left, right, .. } => {
                        assert_eq!(**left, Expr::Number(1.0));
                        match &**right {
                            Expr::Binary { op: BinaryOp::Mul, left, right, .. } => {
                                assert_eq!(**left, Expr::Number(2.0));
                                assert_eq!(**right, Expr::Number(3.0));
                            }
                            other => panic!("expected multiplication, got {:?}", other),
                        }
                    }
                    other => panic!("expected addition at top, got {:?}", other),
                }
            }
            other => panic!("expected let statement, got {:?}", other),
        }
    }

    #[test]
    fn multiplication_binds_tighter_than_addition_general_shape() {
        let stmts = parse_src("1 + 2 * 3;");
        let expr = match &stmts[0] {
            Stmt::Expr(e) => e,
            _ => panic!("expected expr stmt"),
        };
        // Top-level op must be Add, not Mul.
        match expr {
            Expr::Binary { op, .. } => assert_eq!(*op, BinaryOp::Add),
            _ => panic!("expected binary expr"),
        }
    }

    #[test]
    fn parenthesized_expression_overrides_precedence() {
        let stmts = parse_src("(1 + 2) * 3;");
        let expr = match &stmts[0] {
            Stmt::Expr(e) => e,
            _ => panic!("expected expr stmt"),
        };
        match expr {
            Expr::Binary { op: BinaryOp::Mul, left, .. } => match &**left {
                Expr::Binary { op: BinaryOp::Add, .. } => {}
                other => panic!("expected addition inside parens, got {:?}", other),
            },
            other => panic!("expected top-level multiplication, got {:?}", other),
        }
    }

    #[test]
    fn parses_unary_minus_vs_binary_minus() {
        let stmts = parse_src("let x = -5 - 3;");
        match &stmts[0] {
            Stmt::Let { initializer, .. } => match initializer {
                Expr::Binary { op: BinaryOp::Sub, left, right, .. } => {
                    assert_eq!(
                        **left,
                        Expr::Unary {
                            op: UnaryOp::Neg,
                            expr: Box::new(Expr::Number(5.0)),
                            line: 1,
                        }
                    );
                    assert_eq!(**right, Expr::Number(3.0));
                }
                other => panic!("expected subtraction, got {:?}", other),
            },
            other => panic!("expected let, got {:?}", other),
        }
    }

    #[test]
    fn parses_function_decl_with_params_and_return() {
        let stmts = parse_src("fn add(a, b) { return a + b; }");
        match &stmts[0] {
            Stmt::FunctionDecl { name, params, body, .. } => {
                assert_eq!(name, "add");
                assert_eq!(params, &vec!["a".to_string(), "b".to_string()]);
                assert_eq!(body.len(), 1);
                assert!(matches!(body[0], Stmt::Return { .. }));
            }
            other => panic!("expected function decl, got {:?}", other),
        }
    }

    #[test]
    fn parses_if_else() {
        let stmts = parse_src("if (x > 0) { print(1); } else { print(2); }");
        match &stmts[0] {
            Stmt::If { then_branch, else_branch, .. } => {
                assert_eq!(then_branch.len(), 1);
                assert!(else_branch.is_some());
            }
            other => panic!("expected if statement, got {:?}", other),
        }
    }

    #[test]
    fn parses_while_loop() {
        let stmts = parse_src("while (x < 10) { x = x + 1; }");
        match &stmts[0] {
            Stmt::While { body, .. } => assert_eq!(body.len(), 1),
            other => panic!("expected while statement, got {:?}", other),
        }
    }

    #[test]
    fn parses_logical_and_or_precedence() {
        // a || b && c should parse as a || (b && c) since && binds tighter
        let stmts = parse_src("a || b && c;");
        match &stmts[0] {
            Stmt::Expr(Expr::Logical { op: LogicalOp::Or, right, .. }) => {
                assert!(matches!(**right, Expr::Logical { op: LogicalOp::And, .. }));
            }
            other => panic!("expected top-level or, got {:?}", other),
        }
    }

    #[test]
    fn parses_call_expression_with_args() {
        let stmts = parse_src("foo(1, 2 + 3);");
        match &stmts[0] {
            Stmt::Expr(Expr::Call { args, .. }) => assert_eq!(args.len(), 2),
            other => panic!("expected call expr, got {:?}", other),
        }
    }

    #[test]
    fn missing_semicolon_is_parse_error() {
        let err = parse(tokenize("let x = 5").unwrap()).unwrap_err();
        assert!(err.message.contains("';'"));
    }
}
