def sum_range(n: int) -> int:
    total = 0
    for i in range(1, n + 1):
        total = total + i
    return total

print(sum_range(10))
