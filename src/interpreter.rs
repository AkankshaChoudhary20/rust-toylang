//! Tree-walking interpreter with lexically-scoped environments.
//!
//! Closures capture their defining environment by reference
//! (`Rc<RefCell<Scope>>`), so a function created inside another function
//! keeps a live link to that call's locals even after the outer call
//! returns -- this is what makes `make_counter()`-style closures work.

use crate::ast::{BinaryOp, Expr, LogicalOp, Stmt, UnaryOp};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::io::Write;
use std::rc::Rc;

#[derive(Clone)]
pub enum Value {
    Number(f64),
    Str(String),
    Bool(bool),
    Nil,
    Function(Rc<FunctionValue>),
    Native(Rc<NativeFunction>),
}

pub struct NativeFunction {
    pub name: String,
    pub arity: Option<usize>,
    pub func: Box<dyn Fn(&[Value]) -> Result<Value, String>>,
}

pub struct FunctionValue {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub closure: Env,
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => {
                if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e15 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::Str(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Nil => write!(f, "nil"),
            Value::Function(func) => write!(f, "<fn {}>", func.name),
            Value::Native(func) => write!(f, "<native fn {}>", func.name),
        }
    }
}

impl Value {
    fn type_name(&self) -> &'static str {
        match self {
            Value::Number(_) => "number",
            Value::Str(_) => "string",
            Value::Bool(_) => "bool",
            Value::Nil => "nil",
            Value::Function(_) => "function",
            Value::Native(_) => "function",
        }
    }

    fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Nil => false,
            _ => true,
        }
    }

    fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            (Value::Native(a), Value::Native(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeError {
    pub message: String,
    pub line: usize,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Runtime error at line {}: {}", self.line, self.message)
    }
}

fn err<T>(line: usize, message: impl Into<String>) -> Result<T, RuntimeError> {
    Err(RuntimeError {
        message: message.into(),
        line,
    })
}

/// Signal used to unwind the call stack for `return` and for propagating
/// runtime errors out of statement execution, without panicking.
enum Signal {
    Return(Value),
    Error(RuntimeError),
}

impl From<RuntimeError> for Signal {
    fn from(e: RuntimeError) -> Self {
        Signal::Error(e)
    }
}

type ExecResult = Result<(), Signal>;

#[derive(Debug)]
pub struct Scope {
    values: HashMap<String, Value>,
    parent: Option<Env>,
}

pub type Env = Rc<RefCell<Scope>>;

impl fmt::Debug for FunctionValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<fn {}>", self.name)
    }
}

fn new_scope(parent: Option<Env>) -> Env {
    Rc::new(RefCell::new(Scope {
        values: HashMap::new(),
        parent,
    }))
}

fn scope_define(env: &Env, name: &str, value: Value) {
    env.borrow_mut().values.insert(name.to_string(), value);
}

fn scope_get(env: &Env, name: &str, line: usize) -> Result<Value, RuntimeError> {
    if let Some(v) = env.borrow().values.get(name) {
        return Ok(v.clone());
    }
    let parent = env.borrow().parent.clone();
    match parent {
        Some(p) => scope_get(&p, name, line),
        None => err(line, format!("undefined variable '{}'", name)),
    }
}

fn scope_assign(env: &Env, name: &str, value: Value, line: usize) -> Result<(), RuntimeError> {
    if env.borrow().values.contains_key(name) {
        env.borrow_mut().values.insert(name.to_string(), value);
        return Ok(());
    }
    let parent = env.borrow().parent.clone();
    match parent {
        Some(p) => scope_assign(&p, name, value, line),
        None => err(line, format!("undefined variable '{}' (cannot assign before declaring with 'let')", name)),
    }
}

pub struct Interpreter {
    globals: Env,
}

impl Interpreter {
    pub fn new(output: Rc<RefCell<dyn Write>>) -> Self {
        let globals = new_scope(None);

        let print_fn = NativeFunction {
            name: "print".to_string(),
            arity: None,
            func: Box::new(move |args: &[Value]| {
                let rendered: Vec<String> = args.iter().map(|v| v.to_string()).collect();
                let mut out = output.borrow_mut();
                writeln!(out, "{}", rendered.join(" "))
                    .map_err(|error| format!("failed to write output: {}", error))?;
                Ok(Value::Nil)
            }),
        };
        scope_define(&globals, "print", Value::Native(Rc::new(print_fn)));

        Interpreter { globals }
    }

    /// Run a full program (used for script mode and once per REPL line,
    /// reusing the same `globals` scope so state persists across calls).
    pub fn interpret(&mut self, stmts: &[Stmt]) -> Result<(), RuntimeError> {
        for stmt in stmts {
            match self.exec_stmt(stmt, &self.globals.clone()) {
                Ok(()) => {}
                Err(Signal::Return(_)) => {
                    return err(0, "cannot 'return' from top-level code");
                }
                Err(Signal::Error(e)) => return Err(e),
            }
        }
        Ok(())
    }

    fn exec_block(&mut self, stmts: &[Stmt], env: &Env) -> ExecResult {
        for stmt in stmts {
            self.exec_stmt(stmt, env)?;
        }
        Ok(())
    }

    fn exec_stmt(&mut self, stmt: &Stmt, env: &Env) -> ExecResult {
        match stmt {
            Stmt::Let { name, initializer, .. } => {
                let value = self.eval(initializer, env)?;
                scope_define(env, name, value);
                Ok(())
            }
            Stmt::Expr(expr) => {
                self.eval(expr, env)?;
                Ok(())
            }
            Stmt::Block(body) => {
                let inner = new_scope(Some(env.clone()));
                self.exec_block(body, &inner)
            }
            Stmt::If { cond, then_branch, else_branch } => {
                let c = self.eval(cond, env)?;
                if c.is_truthy() {
                    let inner = new_scope(Some(env.clone()));
                    self.exec_block(then_branch, &inner)
                } else if let Some(else_b) = else_branch {
                    let inner = new_scope(Some(env.clone()));
                    self.exec_block(else_b, &inner)
                } else {
                    Ok(())
                }
            }
            Stmt::While { cond, body } => {
                loop {
                    let c = self.eval(cond, env)?;
                    if !c.is_truthy() {
                        break;
                    }
                    let inner = new_scope(Some(env.clone()));
                    self.exec_block(body, &inner)?;
                }
                Ok(())
            }
            Stmt::FunctionDecl { name, params, body, .. } => {
                let func = FunctionValue {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                    // Capture the *current* environment by reference: this is
                    // the closure. Later mutations to variables in `env`
                    // (e.g. `count = count + 1`) are visible through this Rc.
                    closure: env.clone(),
                };
                scope_define(env, name, Value::Function(Rc::new(func)));
                Ok(())
            }
            Stmt::Return { value, .. } => {
                let v = match value {
                    Some(e) => self.eval(e, env)?,
                    None => Value::Nil,
                };
                Err(Signal::Return(v))
            }
        }
    }

    fn call_function(&mut self, callee: &Value, args: Vec<Value>, line: usize) -> Result<Value, RuntimeError> {
        match callee {
            Value::Function(func) => {
                if args.len() != func.params.len() {
                    return err(
                        line,
                        format!(
                            "function '{}' expects {} argument(s) but got {}",
                            func.name,
                            func.params.len(),
                            args.len()
                        ),
                    );
                }
                let call_env = new_scope(Some(func.closure.clone()));
                for (param, arg) in func.params.iter().zip(args.into_iter()) {
                    scope_define(&call_env, param, arg);
                }
                match self.exec_block(&func.body, &call_env) {
                    Ok(()) => Ok(Value::Nil),
                    Err(Signal::Return(v)) => Ok(v),
                    Err(Signal::Error(e)) => Err(e),
                }
            }
            Value::Native(native) => {
                if let Some(arity) = native.arity {
                    if args.len() != arity {
                        return err(
                            line,
                            format!(
                                "function '{}' expects {} argument(s) but got {}",
                                native.name,
                                arity,
                                args.len()
                            ),
                        );
                    }
                }
                (native.func)(&args).map_err(|msg| RuntimeError { message: msg, line })
            }
            other => err(
                line,
                format!("cannot call value of type '{}' (not a function)", other.type_name()),
            ),
        }
    }

    fn eval(&mut self, expr: &Expr, env: &Env) -> Result<Value, RuntimeError> {
        match expr {
            Expr::Number(n) => Ok(Value::Number(*n)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Nil => Ok(Value::Nil),
            Expr::Identifier(name, line) => scope_get(env, name, *line),
            Expr::Assign { name, value, line } => {
                let v = self.eval(value, env)?;
                scope_assign(env, name, v.clone(), *line)?;
                Ok(v)
            }
            Expr::Unary { op, expr, line } => {
                let v = self.eval(expr, env)?;
                match op {
                    UnaryOp::Not => Ok(Value::Bool(!v.is_truthy())),
                    UnaryOp::Neg => match v {
                        Value::Number(n) => Ok(Value::Number(-n)),
                        other => err(*line, format!("cannot negate a {}", other.type_name())),
                    },
                }
            }
            Expr::Logical { op, left, right } => {
                let l = self.eval(left, env)?;
                match op {
                    LogicalOp::Or => {
                        if l.is_truthy() {
                            Ok(l)
                        } else {
                            self.eval(right, env)
                        }
                    }
                    LogicalOp::And => {
                        if !l.is_truthy() {
                            Ok(l)
                        } else {
                            self.eval(right, env)
                        }
                    }
                }
            }
            Expr::Binary { op, left, right, line } => {
                let l = self.eval(left, env)?;
                let r = self.eval(right, env)?;
                self.eval_binary(*op, l, r, *line)
            }
            Expr::Call { callee, args, line } => {
                let callee_val = self.eval(callee, env)?;
                let mut arg_vals = Vec::with_capacity(args.len());
                for a in args {
                    arg_vals.push(self.eval(a, env)?);
                }
                self.call_function(&callee_val, arg_vals, *line)
            }
        }
    }

    fn eval_binary(&self, op: BinaryOp, l: Value, r: Value, line: usize) -> Result<Value, RuntimeError> {
        use BinaryOp::*;
        match op {
            Add => match (l, r) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
                (Value::Str(a), Value::Str(b)) => Ok(Value::Str(a + &b)),
                (a, b) => err(
                    line,
                    format!(
                        "type mismatch: cannot add {} and {}",
                        a.type_name(),
                        b.type_name()
                    ),
                ),
            },
            Sub => numeric_op(l, r, line, "subtract", |a, b| a - b),
            Mul => numeric_op(l, r, line, "multiply", |a, b| a * b),
            Div => match (l, r) {
                (Value::Number(_), Value::Number(b)) if b == 0.0 => {
                    err(line, "division by zero")
                }
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a / b)),
                (a, b) => err(
                    line,
                    format!(
                        "type mismatch: cannot divide {} by {}",
                        a.type_name(),
                        b.type_name()
                    ),
                ),
            },
            Eq => Ok(Value::Bool(l.equals(&r))),
            NotEq => Ok(Value::Bool(!l.equals(&r))),
            Lt => compare(l, r, line, |o| o == std::cmp::Ordering::Less),
            LtEq => compare(l, r, line, |o| o != std::cmp::Ordering::Greater),
            Gt => compare(l, r, line, |o| o == std::cmp::Ordering::Greater),
            GtEq => compare(l, r, line, |o| o != std::cmp::Ordering::Less),
        }
    }
}

fn numeric_op(l: Value, r: Value, line: usize, verb: &str, f: impl Fn(f64, f64) -> f64) -> Result<Value, RuntimeError> {
    match (l, r) {
        (Value::Number(a), Value::Number(b)) => Ok(Value::Number(f(a, b))),
        (a, b) => err(
            line,
            format!(
                "type mismatch: cannot {} {} and {}",
                verb,
                a.type_name(),
                b.type_name()
            ),
        ),
    }
}

fn compare(l: Value, r: Value, line: usize, pred: impl Fn(std::cmp::Ordering) -> bool) -> Result<Value, RuntimeError> {
    match (l, r) {
        (Value::Number(a), Value::Number(b)) => match a.partial_cmp(&b) {
            Some(o) => Ok(Value::Bool(pred(o))),
            None => err(line, "cannot compare NaN"),
        },
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(pred(a.cmp(&b)))),
        (a, b) => err(
            line,
            format!(
                "type mismatch: cannot compare {} and {}",
                a.type_name(),
                b.type_name()
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::run_and_capture;

    #[test]
    fn output_write_errors_are_reported_with_the_call_line() {
        use std::{cell::RefCell, io::{self, Write}, rc::Rc};
        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "reader closed"))
            }
            fn flush(&mut self) -> io::Result<()> { Ok(()) }
        }
        let output = Rc::new(RefCell::new(FailingWriter));
        let mut interpreter = super::Interpreter::new(output);
        let tokens = crate::lexer::tokenize("\nprint(42);").unwrap();
        let program = crate::parser::parse(tokens).unwrap();
        let error = interpreter.interpret(&program).unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("failed to write output"));
        assert!(error.message.contains("reader closed"));
    }

    #[test]
    fn arithmetic_precedence() {
        let out = run_and_capture("print(1 + 2 * 3);").unwrap();
        assert_eq!(out, "7\n");
    }

    #[test]
    fn string_concatenation_and_type_mismatch() {
        let out = run_and_capture(r#"print("a" + "b");"#).unwrap();
        assert_eq!(out, "ab\n");

        let err = run_and_capture(r#"print(1 + "b");"#).unwrap_err();
        assert!(err.contains("type mismatch"), "got: {}", err);
    }

    #[test]
    fn division_by_zero_is_reported_not_panicking() {
        let err = run_and_capture("print(1 / 0);").unwrap_err();
        assert!(err.contains("division by zero"), "got: {}", err);
    }

    #[test]
    fn undefined_variable_is_reported() {
        let err = run_and_capture("print(doesNotExist);").unwrap_err();
        assert!(err.contains("undefined variable"), "got: {}", err);
    }

    #[test]
    fn calling_a_non_function_is_reported() {
        let err = run_and_capture("let x = 5; x();").unwrap_err();
        assert!(err.contains("not a function"), "got: {}", err);
    }

    #[test]
    fn if_else_and_while_and_recursion_fibonacci() {
        let src = r#"
            fn fib(n) {
                if (n < 2) {
                    return n;
                }
                return fib(n - 1) + fib(n - 2);
            }
            print(fib(10));
        "#;
        let out = run_and_capture(src).unwrap();
        assert_eq!(out, "55\n");
    }

    #[test]
    fn closures_capture_and_mutate_outer_scope_across_multiple_calls() {
        // The canonical closure-correctness test: each call to the counter
        // returned by make_counter() must see and mutate the SAME `count`
        // variable, not a fresh copy.
        let src = r#"
            fn make_counter() {
                let count = 0;
                fn increment() {
                    count = count + 1;
                    return count;
                }
                return increment;
            }

            let counter1 = make_counter();
            let counter2 = make_counter();

            print(counter1());
            print(counter1());
            print(counter1());
            print(counter2());
        "#;
        let out = run_and_capture(src).unwrap();
        // counter1 advances independently 1,2,3; counter2 is a *separate*
        // closure instance and starts fresh at 1.
        assert_eq!(out, "1\n2\n3\n1\n");
    }

    #[test]
    fn block_scoping_does_not_leak_outward() {
        let src = r#"
            let x = "outer";
            if (true) {
                let x = "inner";
                print(x);
            }
            print(x);
        "#;
        let out = run_and_capture(src).unwrap();
        assert_eq!(out, "inner\nouter\n");
    }

    #[test]
    fn function_params_do_not_leak_and_variable_inside_function_is_local() {
        let src = r#"
            fn f(n) {
                let local = n * 2;
                return local;
            }
            print(f(5));
            print(local);
        "#;
        let err = run_and_capture(src).unwrap_err();
        assert!(err.contains("undefined variable"), "got: {}", err);
    }

    #[test]
    fn logical_operators_short_circuit_and_and_or() {
        let src = r#"
            fn boom() {
                print("should not run");
                return true;
            }
            print(false && boom());
            print(true || boom());
        "#;
        let out = run_and_capture(src).unwrap();
        assert_eq!(out, "false\ntrue\n");
    }

    #[test]
    fn while_loop_accumulates() {
        let src = r#"
            let i = 0;
            let sum = 0;
            while (i < 5) {
                sum = sum + i;
                i = i + 1;
            }
            print(sum);
        "#;
        let out = run_and_capture(src).unwrap();
        assert_eq!(out, "10\n");
    }

    #[test]
    fn nil_and_booleans_print_correctly() {
        let out = run_and_capture("print(nil); print(true); print(false);").unwrap();
        assert_eq!(out, "nil\ntrue\nfalse\n");
    }
}
