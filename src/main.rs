use std::cell::RefCell;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;
use std::rc::Rc;

use toylang::interpreter::Interpreter;
use toylang::{lexer, parser};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match args.len() {
        1 => {
            run_repl();
            ExitCode::SUCCESS
        }
        2 => run_file(&args[1]),
        _ => {
            eprintln!("Usage: {} [script.toy]", args[0]);
            ExitCode::from(64)
        }
    }
}

fn run_file(path: &str) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Could not read file '{}': {}", path, e);
            return ExitCode::from(66);
        }
    };

    let stdout = Rc::new(RefCell::new(io::stdout()));
    let mut interp = Interpreter::new(stdout);

    let tokens = match lexer::tokenize(&source) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::from(65);
        }
    };
    let program = match parser::parse(tokens) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::from(65);
        }
    };
    match interp.interpret(&program) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", e);
            ExitCode::from(70)
        }
    }
}

fn run_repl() {
    println!("toylang REPL -- Ctrl-D (or an empty 'exit') to quit.");
    let stdin = io::stdin();
    let stdout = Rc::new(RefCell::new(io::stdout()));
    // One Interpreter for the whole REPL session: its `globals` scope is
    // reused across every line, which is what lets `let` bindings and
    // function declarations persist across separate inputs.
    let mut interp = Interpreter::new(stdout);

    let mut buffer = String::new();
    loop {
        if buffer.is_empty() {
            print!("> ");
        } else {
            print!("... ");
        }
        let _ = io::stdout().flush();

        let mut line = String::new();
        let bytes_read = match stdin.read_line(&mut line) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("input error: {}", e);
                break;
            }
        };
        if bytes_read == 0 {
            // EOF (Ctrl-D)
            println!();
            break;
        }

        if buffer.is_empty() && line.trim() == "exit" {
            break;
        }

        buffer.push_str(&line);

        // Accumulate lines until braces balance, so multi-line `fn`/`if`/
        // `while` bodies can be typed interactively.
        if brace_balance(&buffer) > 0 {
            continue;
        }

        let source = std::mem::take(&mut buffer);
        if source.trim().is_empty() {
            continue;
        }

        let tokens = match lexer::tokenize(&source) {
            Ok(t) => t,
            Err(e) => {
                println!("{}", e);
                continue;
            }
        };
        let program = match parser::parse(tokens) {
            Ok(p) => p,
            Err(e) => {
                println!("{}", e);
                continue;
            }
        };
        if let Err(e) = interp.interpret(&program) {
            // Runtime errors are reported and the REPL keeps going, per spec.
            println!("{}", e);
        }
    }
}

/// Count of unmatched `{` in `s` (used to decide whether a REPL line's
/// block is complete). Not comment/string aware beyond a simple skip; good
/// enough for an interactive REPL.
fn brace_balance(s: &str) -> i32 {
    let mut balance = 0;
    let mut in_string = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => in_string = !in_string,
            '\\' if in_string => {
                chars.next();
            }
            '{' if !in_string => balance += 1,
            '}' if !in_string => balance -= 1,
            '/' if !in_string && chars.peek() == Some(&'/') => {
                for c2 in chars.by_ref() {
                    if c2 == '\n' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    balance
}
