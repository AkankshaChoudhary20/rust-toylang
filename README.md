# toylang

A small interpreted scripting language, implemented from scratch in Rust:
a hand-written lexer, a hand-written recursive-descent parser (no
`pest`/`lalrpop`), producing an AST that a tree-walking interpreter
evaluates directly. No external crates are used at all.

## Quick example

```
// examples/closures.toy (excerpt)
fn make_counter() {
    let count = 0;
    fn increment() {
        count = count + 1;
        return count;
    }
    return increment;
}

let counterA = make_counter();
let counterB = make_counter();

print(counterA()); // 1
print(counterA()); // 2
print(counterB()); // 1  <- independent closure state
```

Running it:

```
$ cargo run --release -- examples/closures.toy
1
2
3
1
2
4
Hello, Ada!
Hello, Lin!
```

## Architecture

```
source text
   |  src/lexer.rs   (hand-written, char-by-char scanner)
   v
Vec<Token>
   |  src/parser.rs  (hand-written recursive-descent parser)
   v
AST (src/ast.rs: Expr / Stmt)
   |  src/interpreter.rs (tree-walking evaluator)
   v
side effects (print) + runtime errors
```

- **`src/lexer.rs`** — turns source text into a flat `Vec<Token>`, tracking
  line numbers for later error reporting. Handles numbers, strings (with
  `\n \t \r \\ \" \0` escapes), identifiers/keywords, operators, and
  `// ...` single-line comments. Notably, `-5` lexes as two tokens
  (`Minus`, `Number(5.0)`) rather than folding the sign into the literal —
  the parser is what turns `Minus` immediately before an operand into unary
  negation, which is what correctly distinguishes `5 - 3` (binary) from
  `-5` (unary) using the same tokens.
- **`src/parser.rs`** — a standard recursive-descent / precedence-climbing
  parser. Each precedence level is its own method calling into the next
  tighter one, so `1 + 2 * 3` naturally parses as `1 + (2 * 3)`.
- **`src/ast.rs`** — the `Expr` and `Stmt` node definitions the parser
  builds and the interpreter walks.
- **`src/interpreter.rs`** — evaluates the AST directly against a chain of
  lexical scopes (`Scope { values: HashMap<String, Value>, parent: Option<Env> }`,
  where `Env = Rc<RefCell<Scope>>`). Function values capture the `Env` that
  was current at the point of declaration; because that's a reference
  (`Rc<RefCell<_>>`), not a copy, later mutations to captured variables
  (`count = count + 1`) are visible to the closure — this is what makes the
  `make_counter()` pattern above work correctly.
- **`src/main.rs`** — the two run modes (REPL and script file), described
  below.

## Language spec

Grammar, EBNF-ish (see `src/parser.rs` for the literal recursive-descent
implementation, which mirrors this 1:1):

```
program        := declaration* EOF

declaration    := funDecl | letDecl | statement
funDecl        := "fn" IDENTIFIER "(" params? ")" block
letDecl        := "let" IDENTIFIER "=" expression ";"
params         := IDENTIFIER ( "," IDENTIFIER )*

statement      := exprStmt | ifStmt | whileStmt | returnStmt | block
exprStmt       := expression ";"
ifStmt         := "if" "(" expression ")" block ( "else" ( block | ifStmt ) )?
whileStmt      := "while" "(" expression ")" block
returnStmt     := "return" expression? ";"
block          := "{" declaration* "}"

expression     := assignment
assignment     := IDENTIFIER "=" assignment | logic_or
logic_or       := logic_and ( "||" logic_and )*
logic_and      := equality ( "&&" equality )*
equality       := comparison ( ( "==" | "!=" ) comparison )*
comparison     := term ( ( "<" | "<=" | ">" | ">=" ) term )*
term           := factor ( ( "+" | "-" ) factor )*
factor         := unary ( ( "*" | "/" ) unary )*
unary          := ( "!" | "-" ) unary | call
call           := primary ( "(" arguments? ")" )*
arguments      := expression ( "," expression )*
primary        := NUMBER | STRING | "true" | "false" | "nil"
                 | IDENTIFIER | "(" expression ")"
```

Operator precedence, low to high: `= ` (assignment, right-assoc) <
`||` < `&&` < `== !=` < `< <= > >=` < `+ -` < `* /` < unary `! -` < call `(...)`.
All binary operators are left-associative except assignment.

### Types

- **Numbers** — all numbers are `f64` (e.g. `5`, `3.14`, `-2.5`).
- **Strings** — double-quoted, with `\n \t \r \\ \" \0` escapes. `+`
  concatenates two strings.
- **Booleans** — `true` / `false`.
- **`nil`** — the absence of a value; falsy, like `false`.
- **Functions** — first-class values. A `fn name(...) { ... }` declaration
  binds `name` to a function value in the current scope; that value can be
  returned, stored in a variable, and called like `f(...)`. Closures
  capture their *defining* environment by reference, so nested functions
  see live updates to the outer function's locals (see the counter example
  above).

Truthiness: everything is truthy except `false` and `nil` (including `0`
and `""`, which are truthy).

### Comments

`// rest of the line is ignored`

### Built-ins

- `print(a, b, ...)` — prints its arguments space-separated, followed by a
  newline, and returns `nil`.

## Running it

Two modes, driven purely by argument count:

```
# No args: interactive REPL. State (variables, functions) persists across
# lines. Runtime errors are printed and the REPL keeps going.
$ cargo run --release
toylang REPL -- Ctrl-D (or an empty 'exit') to quit.
> let x = 5;
> print(x * 2);
10
> print(1 + "oops");
Runtime error at line 1: type mismatch: cannot add number and string
> exit

# One arg: run a .toy script file and exit.
$ cargo run --release -- examples/fibonacci.toy
0
1
1
2
3
5
8
13
21
34
```

The REPL accumulates input across lines until braces balance, so a
multi-line `fn` / `if` / `while` can be typed interactively without every
line needing to be self-contained.

## Error handling

Runtime errors (undefined variable, adding a string to a number, division
by zero, calling something that isn't a function, wrong argument count,
etc.) are represented as `Result::Err(RuntimeError { message, line })` and
propagated through the interpreter with `?` — the process never panics on
a bad user script. In script mode, an error is printed to stderr and the
process exits with a non-zero code. In the REPL, the error is printed and
the loop continues, keeping all previously-defined state intact.

Examples:

```
$ echo 'print(1 / 0);' | cargo run --release -- /dev/stdin
Runtime error at line 1: division by zero

$ echo 'print(nope);' | cargo run --release -- /dev/stdin
Runtime error at line 1: undefined variable 'nope'

$ echo 'let x = 5; x();' | cargo run --release -- /dev/stdin
Runtime error at line 1: cannot call value of type 'number' (not a function)
```

## Examples

See `examples/`:

- `fibonacci.toy` — recursive Fibonacci.
- `fizzbuzz.toy` — FizzBuzz 1..20 (toylang has no `%` operator, so the
  script defines its own `remainder()` via a `while` loop).
- `closures.toy` — `make_counter()` / independent closure instances /
  closures capturing a parameter (`make_greeter(name)`).

## Tests

```
$ cargo test
```

- **Lexer unit tests** (`src/lexer.rs`) — token sequences for representative
  source, including `-5` tokenizing as `Minus, Number(5.0)` (not a single
  negative-literal token) so it's distinguishable from `5 - 3` purely by
  how the parser consumes the same token shapes; string escapes; comments;
  line-number tracking; unknown-character and unterminated-string errors.
- **Parser unit tests** (`src/parser.rs`) — AST shape for `let`, `fn`,
  `if`/`else`, `while`, calls, and explicit precedence checks (e.g.
  `1 + 2 * 3` parses with `*` as the inner node, `(1 + 2) * 3` overrides
  it, `a || b && c` parses `&&` as binding tighter than `||`).
- **Interpreter end-to-end tests** (`src/interpreter.rs`, run via
  `run_and_capture` in `src/lib.rs`, which captures `print` output instead
  of writing to real stdout) — recursion (`fib(10) == 55`), the
  closure/counter pattern across **multiple independent instances**,
  block/function scoping (an inner `let` does not leak to an outer scope,
  and a function-local variable is not visible after the call returns),
  short-circuit evaluation of `&&`/`||`, and each runtime error category.

All 32 tests pass; `cargo build --release` and `cargo test` are both
required to be clean before this repo is considered done.

## What's NOT supported (being honest about scope)

This is a small teaching-sized language, not a production one. Explicitly
out of scope:

- No arrays, objects/maps, or any composite/collection data type.
- No `for` loops (only `while`) — see `examples/fizzbuzz.toy` for the
  idiom this pushes you toward.
- No `%` (modulo) operator.
- No static/type checking of any kind — all type errors are caught at
  runtime, not compile time.
- No standard library beyond `print`. No file I/O, no string
  methods/indexing, no math functions.
- No module system / imports; a program is a single file (or a single
  REPL session).
- Numbers are always `f64` — no separate integer type, no overflow
  behavior to speak of.
- No garbage collection tuning needed/present beyond Rust's own `Rc`
  reference counting for closures (so reference cycles between closures
  that capture each other would leak — not a concern for any program this
  language is meant to write).
- No tail-call optimization; deep recursion is bounded by the native Rust
  call stack.
