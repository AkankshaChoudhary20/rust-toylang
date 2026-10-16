//! Hand-written lexer: turns source text into a flat stream of tokens.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Number(f64),
    Str(String),
    Identifier(String),

    // Keywords
    Let,
    Fn,
    If,
    Else,
    While,
    Return,
    True,
    False,
    Nil,

    // Single/double-char operators
    Plus,
    Minus,
    Star,
    Slash,
    Bang,
    Eq,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AndAnd,
    OrOr,

    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Semicolon,

    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub line: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Lex error at line {}: {}", self.line, self.message)
    }
}

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
        }
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            let line = self.line;
            let c = match self.advance() {
                Some(c) => c,
                None => {
                    tokens.push(Token {
                        kind: TokenKind::Eof,
                        line,
                    });
                    break;
                }
            };

            let kind = match c {
                '+' => TokenKind::Plus,
                '-' => TokenKind::Minus,
                '*' => TokenKind::Star,
                '/' => TokenKind::Slash,
                '(' => TokenKind::LParen,
                ')' => TokenKind::RParen,
                '{' => TokenKind::LBrace,
                '}' => TokenKind::RBrace,
                ',' => TokenKind::Comma,
                ';' => TokenKind::Semicolon,
                '!' => {
                    if self.match_char('=') {
                        TokenKind::NotEq
                    } else {
                        TokenKind::Bang
                    }
                }
                '=' => {
                    if self.match_char('=') {
                        TokenKind::EqEq
                    } else {
                        TokenKind::Eq
                    }
                }
                '<' => {
                    if self.match_char('=') {
                        TokenKind::LtEq
                    } else {
                        TokenKind::Lt
                    }
                }
                '>' => {
                    if self.match_char('=') {
                        TokenKind::GtEq
                    } else {
                        TokenKind::Gt
                    }
                }
                '&' => {
                    if self.match_char('&') {
                        TokenKind::AndAnd
                    } else {
                        return Err(LexError {
                            message: "unexpected character '&' (did you mean '&&'?)".to_string(),
                            line,
                        });
                    }
                }
                '|' => {
                    if self.match_char('|') {
                        TokenKind::OrOr
                    } else {
                        return Err(LexError {
                            message: "unexpected character '|' (did you mean '||'?)".to_string(),
                            line,
                        });
                    }
                }
                '"' => self.string(line)?,
                c if c.is_ascii_digit() => self.number(c, line)?,
                c if is_ident_start(c) => self.identifier(c),
                other => {
                    return Err(LexError {
                        message: format!("unexpected character '{}'", other),
                        line,
                    });
                }
            };

            tokens.push(Token { kind, line });
        }
        Ok(tokens)
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\r') => {
                    self.pos += 1;
                }
                Some('\n') => {
                    self.pos += 1;
                    self.line += 1;
                }
                Some('/') if self.peek_at(1) == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn string(&mut self, start_line: usize) -> Result<TokenKind, LexError> {
        let mut s = String::new();
        loop {
            match self.advance() {
                None => {
                    return Err(LexError {
                        message: "unterminated string literal".to_string(),
                        line: start_line,
                    });
                }
                Some('"') => break,
                Some('\n') => {
                    return Err(LexError {
                        message: "unterminated string literal (newline in string)".to_string(),
                        line: start_line,
                    });
                }
                Some('\\') => match self.advance() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('r') => s.push('\r'),
                    Some('\\') => s.push('\\'),
                    Some('"') => s.push('"'),
                    Some('0') => s.push('\0'),
                    Some(other) => {
                        return Err(LexError {
                            message: format!("unknown escape sequence '\\{}'", other),
                            line: self.line,
                        });
                    }
                    None => {
                        return Err(LexError {
                            message: "unterminated string literal".to_string(),
                            line: start_line,
                        });
                    }
                },
                Some(c) => s.push(c),
            }
        }
        Ok(TokenKind::Str(s))
    }

    fn number(&mut self, first: char, line: usize) -> Result<TokenKind, LexError> {
        let mut s = String::new();
        s.push(first);
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        // fractional part
        if self.peek() == Some('.') && self.peek_at(1).map(|c| c.is_ascii_digit()).unwrap_or(false)
        {
            s.push('.');
            self.pos += 1;
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    s.push(c);
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        match s.parse::<f64>() {
            Ok(value) if value.is_finite() => Ok(TokenKind::Number(value)),
            _ => Err(LexError {
                message: "numeric literal is outside the finite number range".into(),
                line,
            }),
        }
    }

    fn identifier(&mut self, first: char) -> TokenKind {
        let mut s = String::new();
        s.push(first);
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                s.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        match s.as_str() {
            "let" => TokenKind::Let,
            "fn" => TokenKind::Fn,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "return" => TokenKind::Return,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "nil" => TokenKind::Nil,
            _ => TokenKind::Identifier(s),
        }
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(source).tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        tokenize(src)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn tokenizes_let_statement() {
        let toks = kinds("let x = 5;");
        assert_eq!(
            toks,
            vec![
                TokenKind::Let,
                TokenKind::Identifier("x".to_string()),
                TokenKind::Eq,
                TokenKind::Number(5.0),
                TokenKind::Semicolon,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn negative_number_is_minus_then_number_not_a_single_token() {
        // Lexer intentionally does NOT fold sign into the number literal;
        // the parser is responsible for turning `Minus, Number` into unary negation.
        let toks = kinds("-5");
        assert_eq!(toks, vec![TokenKind::Minus, TokenKind::Number(5.0), TokenKind::Eof]);
    }

    #[test]
    fn subtraction_vs_negative_literal_tokenize_identically_in_shape() {
        let sub = kinds("5 - 3");
        assert_eq!(
            sub,
            vec![
                TokenKind::Number(5.0),
                TokenKind::Minus,
                TokenKind::Number(3.0),
                TokenKind::Eof
            ]
        );

        let neg = kinds("5 + -3");
        assert_eq!(
            neg,
            vec![
                TokenKind::Number(5.0),
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Number(3.0),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn rejects_overflowing_numeric_literals() {
        let source = format!("\nlet huge = {};", "9".repeat(400));
        let error = tokenize(&source).unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("finite number range"));
        assert!(tokenize(&"9".repeat(300)).is_ok());
    }

    #[test]
    fn tokenizes_float_numbers() {
        let toks = kinds("3.14");
        assert_eq!(toks, vec![TokenKind::Number(3.14), TokenKind::Eof]);
    }

    #[test]
    fn tokenizes_string_with_escapes() {
        let toks = kinds(r#""hello\nworld\t\"quoted\"""#);
        assert_eq!(
            toks,
            vec![
                TokenKind::Str("hello\nworld\t\"quoted\"".to_string()),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn unterminated_string_is_an_error() {
        let err = tokenize("\"hello").unwrap_err();
        assert_eq!(err.line, 1);
    }

    #[test]
    fn tokenizes_operators_and_keywords() {
        let toks = kinds("== != <= >= < > && || ! fn if else while return true false nil");
        assert_eq!(
            toks,
            vec![
                TokenKind::EqEq,
                TokenKind::NotEq,
                TokenKind::LtEq,
                TokenKind::GtEq,
                TokenKind::Lt,
                TokenKind::Gt,
                TokenKind::AndAnd,
                TokenKind::OrOr,
                TokenKind::Bang,
                TokenKind::Fn,
                TokenKind::If,
                TokenKind::Else,
                TokenKind::While,
                TokenKind::Return,
                TokenKind::True,
                TokenKind::False,
                TokenKind::Nil,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn skips_comments() {
        let toks = kinds("let x = 1; // this is a comment\nlet y = 2;");
        assert_eq!(
            toks,
            vec![
                TokenKind::Let,
                TokenKind::Identifier("x".to_string()),
                TokenKind::Eq,
                TokenKind::Number(1.0),
                TokenKind::Semicolon,
                TokenKind::Let,
                TokenKind::Identifier("y".to_string()),
                TokenKind::Eq,
                TokenKind::Number(2.0),
                TokenKind::Semicolon,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn tracks_line_numbers_across_newlines() {
        let toks = tokenize("let x = 1;\nlet y = 2;\n").unwrap();
        let let_lines: Vec<usize> = toks
            .iter()
            .filter(|t| t.kind == TokenKind::Let)
            .map(|t| t.line)
            .collect();
        assert_eq!(let_lines, vec![1, 2]);
    }

    #[test]
    fn unknown_character_is_an_error() {
        let err = tokenize("let x = 5 @ 3;").unwrap_err();
        assert_eq!(err.line, 1);
    }
}
