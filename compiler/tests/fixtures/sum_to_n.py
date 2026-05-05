def sum_to_n(n: int) -> int:
    result = 0
    i = 1
    while i <= n:
        result = result + i
        i = i + 1
    return result

print(sum_to_n(10))
