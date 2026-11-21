#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

__attribute__((noinline))
static uint64_t subtract_wide_product(uint64_t *row, size_t n, const uint64_t *v,
                                      size_t v_len, uint64_t q) {
    if (v_len < n) {
        abort();
    }
    uint64_t borrow = 0;
    for (size_t i = 0; i < n; ++i) {
        __uint128_t total = (__uint128_t)q * v[i] + borrow + (uint64_t)~row[i];
        row[i] = ~(uint64_t)total;
        borrow = (uint64_t)(total >> 64);
    }
    return borrow;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        return 1;
    }
    uint64_t count = strtoull(argv[1], NULL, 10);
    uint64_t guess = strtoull(argv[2], NULL, 10);
    uint64_t row[1024], divisor[1024];
    for (size_t i = 0; i < 1024; ++i) {
        row[i] = UINT64_MAX - i;
        divisor[i] = UINT64_MAX ^ (i * UINT64_C(15261831));
    }
    uint64_t borrow = 0;
    for (uint64_t i = 0; i < count; ++i) {
        borrow = subtract_wide_product(row, 1024, divisor, 1024, guess);
    }
    uint64_t checksum = 0;
    for (size_t i = 0; i < 1024; ++i) {
        checksum = checksum * 31 + row[i];
    }
    printf("%" PRIu64 " %" PRIu64 "\n", borrow, checksum);
    return 0;
}
