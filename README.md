# pyferro

A Python-to-native compiler written in Rust. Parses a typed subset of Python, validates it, compiles it to LLVM IR via `inkwell`, runs optimizations, and links a native binary using the system linker.

## Features

- **Types**: `int` (i64) and `bool` (i1)
- **Arithmetic**: `+`, `-`, `*`, `/`
- **Comparisons**: `<`, `>`, `<=`, `>=`, `==`, `!=`
- **Boolean operators**: `and`, `or`, `not`
- **Control flow**: `if/else`, `while`, `for range()`
- **Functions**: typed definitions, mutual calls, recursion
- **Builtin**: `print(x)` — prints int or bool to stdout
- **Optimization**: LLVM `default<O2>` pass pipeline
- **CLI**: compile to binary, emit LLVM IR, or produce a linkable object file

## Supported Python

All arguments and return types must be annotated as `int` or `bool`. Every control-flow path must return.

```python
def factorial(n: int) -> int:
    result = 1
    i = 1
    while i <= n:
        result = result * i
        i = i + 1
    return result

factorial(10)
```

```python
def fib(n: int) -> int:
    if n <= 1:
        return n
    else:
        return fib(n - 1) + fib(n - 2)

print(fib(35))
```

```python
def sum_range(n: int) -> int:
    total = 0
    for i in range(1, n + 1):
        total = total + i
    return total

print(sum_range(100))
```

## Prerequisites

- **Rust** — [install](https://www.rust-lang.org/tools/install)
- **LLVM 22** — required by `inkwell`

### Install LLVM 22

**macOS (Homebrew)**
```bash
brew install llvm@22
# Add to ~/.zshrc:
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
export LDFLAGS="-L/opt/homebrew/opt/llvm/lib"
export CPPFLAGS="-I/opt/homebrew/opt/llvm/include"
```

**Linux (Ubuntu/Debian)**
```bash
wget https://apt.llvm.org/llvm.sh && chmod +x llvm.sh && sudo ./llvm.sh 22
```

**Arch Linux**
```bash
sudo pacman -S llvm clang
```

**Windows**
```powershell
winget install LLVM.LLVM
```

## Build

```bash
cd compiler
cargo build --release
```

The `pyllvm` binary is at `compiler/target/release/pyllvm`.

## Usage

### Compile to native binary
```bash
./target/release/pyllvm path/to/program.py
# Produces program.o and program (executable)
```

### Custom output name
```bash
./target/release/pyllvm path/to/program.py -o my_program
```

### Emit LLVM IR
```bash
./target/release/pyllvm path/to/program.py --emit-ir
# Writes program.ll
```

### Library mode (no top-level call)
If the file has no top-level function call, `pyllvm` skips linking and emits only the object file, ready for external linking.

## Tests

```bash
cd compiler
cargo test
```

| Suite | Command |
|---|---|
| Semantic unit tests | `cargo test --lib` |
| IR snapshot tests | `cargo test --test ir_snapshot_tests` |
| End-to-end tests | `cargo test --test e2e_tests` |

End-to-end tests compile fixture `.py` files, run the resulting binary, and assert stdout.

## Benchmark

```bash
./benchmark.sh          # default N=100_000_000
./benchmark.sh 50000000 # custom N
```

Compares `pyllvm`-compiled binaries against CPython. Reports median CPU user time and speedup ratio.

## Architecture

```
Python source
    │
    ▼
parser.rs   — rustpython-parser → AST
    │
    ▼
semantic.rs — type checks, return-path analysis, unknown-call detection
    │
    ▼
codegen.rs  — inkwell: AST → LLVM IR (two-pass: signatures then bodies)
    │
    ▼
backend.rs  — target machine init, default<O2> passes, object emit, cc link
```

## License

MIT
