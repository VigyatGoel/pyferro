def abs_val(n: int) -> int:
    if n < 0:
        return -n
    return n

def factorial(n: int) -> int:
    result = 1
    i = 2
    while i <= n:
        result = result * i
        i = i + 1
    return result

def sum_squares(n: int) -> int:
    total = 0
    for i in range(1, n + 1):
        total = total + i * i
    return total

def max_val(a: int, b: int) -> int:
    if a > b:
        return a
    return b

def clamp(val: int, lo: int, hi: int) -> int:
    if val < lo:
        return lo
    if val > hi:
        return hi
    return val

def run_all(dummy: int) -> int:
    print(abs_val(-42))
    print(abs_val(7))
    print(factorial(6))
    print(sum_squares(4))
    print(max_val(10, 20))
    print(clamp(5, 1, 10))
    print(clamp(-5, 1, 10))
    print(clamp(15, 1, 10))
    return 0

run_all(0)
