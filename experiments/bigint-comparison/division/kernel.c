#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

__attribute__((noinline))
static uint64_t complement_row(uint64_t *row, size_t n,
                               const uint64_t *v, size_t vn, uint64_t guess) {
    uint64_t borrow = 0;
    for (size_t i = 0; i < n; ++i) {
        if (i >= vn)
            abort();
        const uint64_t total = guess * v[i] + borrow + (row[i] ^ UINT32_MAX);
        row[i] = (total ^ UINT32_MAX) & UINT32_MAX;
        borrow = total >> 32;
    }
    return borrow;
}

int main(int argc, char **argv) {
    if (argc != 3)
        return 1;
    const long count = strtol(argv[2], NULL, 10);
    uint64_t row[2050];
    uint64_t v[2048];
    for (size_t i = 0; i < 2050; ++i)
        row[i] = UINT32_MAX - i;
    for (size_t i = 0; i < 2048; ++i)
        v[i] = UINT32_MAX ^ ((i * UINT64_C(15261831)) & UINT32_MAX);

    uint64_t borrow = 0;
    for (long k = 0; k < count; ++k)
        borrow = complement_row(row + 1, 2048, v, 2048, UINT32_MAX - 2);
    uint64_t checksum = 0;
    for (size_t i = 0; i < 2050; ++i)
        checksum = checksum * 31 + row[i];
    printf("%" PRIu64 " %" PRIu64 "\n", borrow, checksum);
    return 0;
}
