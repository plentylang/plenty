#include <stdbool.h>
#include <stdint.h>

enum outcome { FALSE, TRUE, OVERFLOW };

enum outcome compare_nearby(bool present, int64_t prior, int64_t i, int64_t k) {
    if (!present) {
        return FALSE;
    }
    int64_t distance;
    if (__builtin_sub_overflow(i, prior, &distance)) {
        return OVERFLOW;
    }
    return distance <= k ? TRUE : FALSE;
}
