#!/usr/bin/env bash
# Benchmark pyferro compiled binaries vs CPython.
# Usage: ./benchmark.sh [N]   (N = loop iterations, default 100_000_000)
#
# Uses CPU user time (not wall clock) for accuracy.
# Each benchmark is run RUNS times and the median is taken.

set -euo pipefail

N=${1:-100000000}
RUNS=5
REPO_ROOT="$(cd "$(dirname "$0")" && pwd)"
COMPILER="$REPO_ROOT/compiler"
PYLLVM="$COMPILER/target/release/pyferro"
BENCH_DIR="$(mktemp -d)"
trap 'rm -rf "$BENCH_DIR"' EXIT

# ── helpers ──────────────────────────────────────────────────────────────────

# Run a command once, return CPU user time in seconds.
time_once() {
    /usr/bin/time -p "$@" > /dev/null 2> "$BENCH_DIR/time_output.txt"
    grep user "$BENCH_DIR/time_output.txt" | python3 -c "
import sys
line = sys.stdin.read().strip()
print(f'{float(line.split()[1]):.3f}')
"
}

# Run command RUNS times, return the median user time.
timeit() {
    local times=()
    local i
    for i in $(seq 1 $RUNS); do
        times+=("$(time_once "$@")")
    done
    printf '%s\n' "${times[@]}" | python3 -c "
import sys, statistics
vals = [float(l.strip()) for l in sys.stdin if l.strip()]
print(f'{statistics.median(vals):.3f}')
"
}

# Compute speedup ratio, guard against division by zero.
speedup() {
    python3 -c "
llvm=float('$1'); py=float('$2')
if llvm == 0:
    print('>999')
else:
    print(f'{py/llvm:.1f}')
"
}

# Compile, time, and print one benchmark row — all output suppressed.
# Usage: run_bench "label" "name" "$SRC"
run_bench() {
    local label="$1" name="$2" src="$3"
    # Print label immediately so user sees progress
    printf "  %-28s  " "$label"
    # Compile silently
    echo "$src" > "$BENCH_DIR/$name.py"
    "$PYLLVM" "$BENCH_DIR/$name.py" --output "$BENCH_DIR/$name" > /dev/null 2>&1
    # Time both
    local t_llvm t_py spdup
    t_llvm=$(timeit "$BENCH_DIR/$name")
    t_py=$(timeit python3 "$BENCH_DIR/$name.py")
    spdup=$(speedup "$t_llvm" "$t_py")
    # Print result on same line
    printf "%7s s  %7s s  %8sx\n" "$t_llvm" "$t_py" "$spdup"
}

# ── build ─────────────────────────────────────────────────────────────────────

echo
echo "Building pyferro (release)..."
cargo build --release --manifest-path "$COMPILER/Cargo.toml" 2>&1 | tail -1

echo
echo "  N             : $N"
echo "  Timing        : CPU user time, median of $RUNS runs"
echo
echo "  ┌──────────────────────────────────────────────────────────────┐"
printf "  │  %-28s  %7s    %7s    %7s  │\n" "Benchmark" "pyferro" "CPython" "Speedup"
echo "  ├──────────────────────────────────────────────────────────────┤"

# ── benchmark cases ───────────────────────────────────────────────────────────

# Wrap each row in the table border
bench_row() {
    printf "  │"
    run_bench "$@"
    # move cursor back and add closing border — simpler: just print row then border
}

run_bench "while loop sum" "while_sum" "
def sum_to_n(n: int) -> int:
    result = 0
    i = 1
    while i <= n:
        result = result + i
        i = i + 1
    return result
print(sum_to_n($N))
"

run_bench "for loop sum (range)" "for_sum" "
def sum_range(n: int) -> int:
    total = 0
    for i in range(1, n + 1):
        total = total + i
    return total
print(sum_range($N))
"

FIB_REPS=1000000
run_bench "fib(40) x${FIB_REPS}" "fibonacci" "
def fib(n: int) -> int:
    a = 0
    b = 1
    i = 0
    while i < n:
        tmp = b
        b = a + b
        a = tmp
        i = i + 1
    return a

def bench(reps: int) -> int:
    result = 0
    i = 0
    while i < reps:
        result = fib(40)
        i = i + 1
    return result

print(bench($FIB_REPS))
"

NEST_N=$(python3 -c "import math; print(int(math.sqrt($N / 10)))")
run_bench "nested loops (n=$NEST_N)" "nested_loop" "
def nested(n: int) -> int:
    total = 0
    i = 0
    while i < n:
        j = 0
        while j < n:
            total = total + i * j
            j = j + 1
        i = i + 1
    return total
print(nested($NEST_N))
"

run_bench "recursive fib(35)" "fib_recursive" "
def fib(n: int) -> int:
    if n <= 1:
        return n
    else:
        return fib(n - 1) + fib(n - 2)

print(fib(35))
"

echo "  └──────────────────────────────────────────────────────────────┘"
echo
