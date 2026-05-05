def sum_evens(n: int) -> int:
    total = 0
    for i in range(0, n, 2):
        total = total + i
    return total

print(sum_evens(10))
