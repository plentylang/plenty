#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

__attribute__((noinline))
static uint64_t divide_render_chunk(uint64_t *words, size_t n, uint64_t divisor) {
    uint64_t remainder = 0;
    while (n > 0) {
        --n;
        uint64_t combined = (remainder << 32) | words[n];
        uint64_t quotient = combined / divisor;
        words[n] = quotient;
        remainder = combined - quotient * divisor;
    }
    return remainder;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        return 1;
    }
    uint64_t count = strtoull(argv[1], NULL, 10);
    uint64_t divisor = strtoull(argv[2], NULL, 10);
    uint64_t words[2048];
    for (size_t i = 0; i < 2048; ++i) {
        words[i] = UINT32_MAX - i;
    }
    uint64_t checksum = 0;
    for (uint64_t i = 0; i < count; ++i) {
        words[2047] = UINT32_MAX - i;
        uint64_t remainder = divide_render_chunk(words, 2048, divisor);
        checksum = checksum * 31 + remainder;
    }
    for (size_t i = 0; i < 2048; ++i) {
        checksum = checksum * 31 + words[i];
    }
    printf("%" PRIu64 "\n", checksum);
    return 0;
}
