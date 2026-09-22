//! toylang: a small interpreted scripting language.
//!
//! Pipeline: source text -> [`lexer`] -> tokens -> [`parser`] -> [`ast`]
//! -> [`interpreter`] (tree-walking evaluation).

pub mod ast;
pub mod interpreter;
pub mod lexer;
pub mod parser;

use interpreter::{Interpreter, RuntimeError};
use std::cell::RefCell;
use std::rc::Rc;

/// Lex + parse + interpret `source` against a fresh interpreter, capturing
/// everything written by `print(...)` and returning it as a String.
///
/// Used by end-to-end tests so they can assert on program output without
/// touching real stdout.
pub fn run_and_capture(source: &str) -> Result<String, String> {
    let buf: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
    let mut interp = Interpreter::new(buf.clone());

    let tokens = lexer::tokenize(source).map_err(|e| e.to_string())?;
    let program = parser::parse(tokens).map_err(|e| e.to_string())?;
    interp.interpret(&program).map_err(|e: RuntimeError| e.to_string())?;

    let bytes = buf.borrow().clone();
    Ok(String::from_utf8_lossy(&bytes).to_string())
}
