#include <assert.h>
#include <immintrin.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

typedef __uint128_t u128;
typedef __m256d Vec;
enum { MAX_PRIMES = 6, MAX_WORDS = 6, TRUNCATION_BLOCK = 256 };
typedef enum { FLAT, BLOCKED, PACKED, TRUNCATED } Mode;

typedef struct {
    uint64_t modulus;
    uint64_t witness;
} Prime;

static const Prime primes[MAX_PRIMES] = {
    {1125844072267777, 5},
    /* Each witness verifies a Proth prime. */
    {1125818302464001, 7},
    {1125809712529409, 3},
    {1125629323902977, 3},
    {1125122517762049, 17},
    {1124907769397249, 3},
};

typedef struct {
    uint64_t p;
    double reciprocal;
    double *forward;
    double *inverse;
    double imag;
    double inv_length;
    double radix64;
    double radix96;
} Field;

typedef struct {
    size_t n;
    size_t na;
    size_t nb;
    unsigned bits;
    unsigned count;
} Profile;

typedef struct {
    Profile profile;
    Field fields[MAX_PRIMES];
    double inverse[MAX_PRIMES][MAX_PRIMES];
    double *residues;
    double *right;
} Plan;

static uint64_t power(uint64_t value, uint64_t exponent, uint64_t prime) {
    uint64_t result = 1;
    for (; exponent; exponent >>= 1, value = (u128)value * value % prime) {
        if (exponent & 1)
            result = (u128)result * value % prime;
    }
    return result;
}

static size_t ceil_pow2(size_t n) {
    size_t result = 1;
    while (result < n)
        result *= 2;
    return result;
}

static unsigned log2_pow2(size_t n) { return __builtin_ctzll(n); }

static Profile select_profile(size_t a, size_t b) {
    static const struct {
        unsigned count, bits;
    } options[] = {
        {4, 80},  {4, 84},  {4, 88},  {4, 92},  {5, 108},
        {5, 112}, {5, 116}, {6, 132}, {6, 136}, {6, 140},
    };
    Profile best = {0};
    size_t best_cost = SIZE_MAX;
    for (size_t i = 0; i < sizeof(options) / sizeof(*options); ++i) {
        unsigned bits = options[i].bits, count = options[i].count;
        size_t na = (64 * a + bits - 1) / bits;
        size_t nb = (64 * b + bits - 1) / bits;
        size_t short_len = na < nb ? na : nb;
        // Each prime exceeds 2^49. This bounds all integer coefficients.
        if (49 * count < 2 * bits + log2_pow2(ceil_pow2(short_len)))
            continue;
        size_t n = ceil_pow2(na + nb - 1);
        if (n < 4)
            n = 4;
        size_t cost = count * n * log2_pow2(n);
        if (cost < best_cost || (cost == best_cost && bits > best.bits)) {
            best = (Profile){n, na, nb, bits, count};
            best_cost = cost;
        }
    }
    assert(best.n && best.n <= (1ULL << 32));
    return best;
}

static void *allocate(size_t count, size_t size) {
    void *memory = calloc(count, size);
    assert(memory);
    return memory;
}

static void fill_roots(double *table, size_t count, uint64_t root, const Field *f);

static Plan make_plan(size_t a, size_t b, Mode mode) {
    Plan plan = {.profile = select_profile(a, b)};
    size_t n = plan.profile.n;
    plan.residues = allocate(plan.profile.count * n, sizeof(double));
    plan.right = allocate((mode >= PACKED ? plan.profile.count : 1) * n, sizeof(double));
    for (unsigned i = 0; i < plan.profile.count; i++) {
        uint64_t p = primes[i].modulus;
        uint64_t g = primes[i].witness;
        // Proth's theorem proves primality and this witness supplies
        // a full-order root for each transform length.
        assert((p & UINT64_C(0xffffffff)) == 1 && p > (1ULL << 49) && p < (1ULL << 50));
        assert(power(g, (p - 1) / 2, p) == p - 1);
        Field *f = &plan.fields[i];
        f->p = p;
        f->reciprocal = 1.0 / p;
        f->forward = allocate(n - 1, sizeof(double));
        f->inverse = allocate(n - 1, sizeof(double));
        f->imag = power(g, (p - 1) / 4, p);
        f->inv_length = power(n, p - 2, p);
        f->radix64 = (UINT64_MAX % p + 1) % p;
        if (mode >= PACKED)
            f->radix96 = power(2, 96, p);
        for (size_t h = 1; h < n; h *= 2) {
            uint64_t w = power(g, (p - 1) / (2 * h), p);
            uint64_t winv = power(w, p - 2, p);
            fill_roots(f->forward + h - 1, h, w, f);
            fill_roots(f->inverse + h - 1, h, winv, f);
        }
        for (unsigned j = 0; j < i; j++)
            plan.inverse[i][j] = power(primes[j].modulus % p, p - 2, p);
    }
    return plan;
}

static void free_plan(Plan *plan) {
    for (unsigned i = 0; i < plan->profile.count; i++) {
        free(plan->fields[i].forward);
        free(plan->fields[i].inverse);
    }
    free(plan->residues);
    free(plan->right);
}

static inline Vec wrap_upper(Vec a, Vec p) {
    return _mm256_sub_pd(a, _mm256_and_pd(_mm256_cmp_pd(a, p, _CMP_GE_OQ), p));
}

static inline Vec wrap_lower(Vec a, Vec p) {
    return _mm256_add_pd(a, _mm256_and_pd(_mm256_cmp_pd(a, _mm256_setzero_pd(), _CMP_LT_OQ), p));
}

static inline Vec add(Vec a, Vec b, Vec p) { return wrap_upper(_mm256_add_pd(a, b), p); }

static inline Vec sub(Vec a, Vec b, Vec p) { return wrap_lower(_mm256_sub_pd(a, b), p); }

static inline Vec mul(Vec a, Vec b, Vec p, Vec reciprocal) {
    // With a,b < p < 2^50, the error in evaluating (a*b)/p
    // is < 0.376. Rounding to nearest adds at most 0.5; the
    // two FMA differences are exact integers and |result| < p.
    Vec hi = _mm256_mul_pd(a, b);
    Vec lo = _mm256_fmsub_pd(a, b, hi);
    Vec q = _mm256_round_pd(_mm256_mul_pd(hi, reciprocal),
                            _MM_FROUND_TO_NEAREST_INT | _MM_FROUND_NO_EXC);
    return wrap_lower(_mm256_add_pd(_mm256_fnmadd_pd(q, p, hi), lo), p);
}

static void fill_roots(double *table, size_t count, uint64_t root, const Field *f) {
    uint64_t square = (u128)root * root % f->p;
    if (count < 4) {
        table[0] = 1;
        if (count == 2)
            table[1] = root;
        return;
    }
    uint64_t cube = (u128)square * root % f->p;
    uint64_t fourth = (u128)square * square % f->p;
    Vec values = _mm256_setr_pd(1.0, root, square, cube);
    Vec step = _mm256_set1_pd(fourth);
    Vec p = _mm256_set1_pd(f->p), r = _mm256_set1_pd(f->reciprocal);
    for (size_t i = 0; i < count; i += 4) {
        _mm256_storeu_pd(table + i, values);
        values = mul(values, step, p, r);
    }
}

static inline double mul_scalar(double a, double b, const Field *f) {
    double hi = a * b;
    double lo = __builtin_fma(a, b, -hi);
    double q = _mm_cvtsd_f64(_mm_round_sd(_mm_setzero_pd(), _mm_set_sd(hi * f->reciprocal),
                                          _MM_FROUND_TO_NEAREST_INT | _MM_FROUND_NO_EXC));
    double r = __builtin_fma(-q, (double)f->p, hi) + lo;
    return r < 0 ? r + f->p : r;
}

static inline void scalar_stage(double *data, size_t h, const Field *f, bool inverse) {
    const double *roots = (inverse ? f->inverse : f->forward) + h - 1;
    for (size_t j = 0; j < h; j++) {
        double a = data[j], b = data[j + h], w = roots[j];
        if (inverse)
            b = mul_scalar(b, w, f);
        double sum = a + b, diff = a - b;
        data[j] = sum >= f->p ? sum - f->p : sum;
        diff = diff < 0 ? diff + f->p : diff;
        data[j + h] = inverse ? diff : mul_scalar(diff, w, f);
    }
}

static void small_transform(double *data, size_t n, const Field *f, bool inverse) {
    for (size_t h = inverse ? 1 : n / 2; h && h < n; h = inverse ? 2 * h : h / 2)
        for (size_t start = 0; start < n; start += 2 * h)
            scalar_stage(data + start, h, f, inverse);
}

static inline void pair_stage(double *data, size_t h, const Field *f, bool inverse) {
    const double *roots = (inverse ? f->inverse : f->forward) + h - 1;
    Vec p = _mm256_set1_pd(f->p), r = _mm256_set1_pd(f->reciprocal);
    for (size_t j = 0; j < h; j += 4) {
        Vec a = _mm256_loadu_pd(data + j);
        Vec b = _mm256_loadu_pd(data + j + h);
        Vec w = _mm256_loadu_pd(roots + j);
        if (inverse)
            b = mul(b, w, p, r);
        _mm256_storeu_pd(data + j, add(a, b, p));
        Vec difference = sub(a, b, p);
        _mm256_storeu_pd(data + j + h, inverse ? difference : mul(difference, w, p, r));
    }
}

static inline void fused_stage(double *data, size_t h, const Field *f, bool inverse) {
    const double *roots = inverse ? f->inverse : f->forward;
    Vec p = _mm256_set1_pd(f->p), r = _mm256_set1_pd(f->reciprocal);
    for (size_t j = 0; j < h; j += 4) {
        Vec a = _mm256_loadu_pd(data + j);
        Vec b = _mm256_loadu_pd(data + h + j);
        Vec c = _mm256_loadu_pd(data + 2 * h + j);
        Vec d = _mm256_loadu_pd(data + 3 * h + j);
        Vec w = _mm256_loadu_pd(roots + 2 * h - 1 + j);
        Vec w2 = _mm256_loadu_pd(roots + h - 1 + j);
        Vec wi = _mm256_loadu_pd(roots + 3 * h - 1 + j);
        if (inverse) {
            b = mul(b, w2, p, r);
            d = mul(d, w2, p, r);
            Vec ab = add(a, b, p), cd = add(c, d, p);
            Vec da = sub(a, b, p), dc = sub(c, d, p);
            cd = mul(cd, w, p, r);
            dc = mul(dc, wi, p, r);
            a = add(ab, cd, p);
            b = add(da, dc, p);
            c = sub(ab, cd, p);
            d = sub(da, dc, p);
        } else {
            Vec ac = add(a, c, p), bd = add(b, d, p);
            Vec da = mul(sub(a, c, p), w, p, r);
            Vec db = mul(sub(b, d, p), wi, p, r);
            a = add(ac, bd, p);
            b = mul(sub(ac, bd, p), w2, p, r);
            c = add(da, db, p);
            d = mul(sub(da, db, p), w2, p, r);
        }
        _mm256_storeu_pd(data + j, a);
        _mm256_storeu_pd(data + h + j, b);
        _mm256_storeu_pd(data + 2 * h + j, c);
        _mm256_storeu_pd(data + 3 * h + j, d);
    }
}

static inline void transpose4(Vec *a, Vec *b, Vec *c, Vec *d) {
    Vec ab0 = _mm256_unpacklo_pd(*a, *b), ab1 = _mm256_unpackhi_pd(*a, *b);
    Vec cd0 = _mm256_unpacklo_pd(*c, *d), cd1 = _mm256_unpackhi_pd(*c, *d);
    *a = _mm256_permute2f128_pd(ab0, cd0, 0x20);
    *b = _mm256_permute2f128_pd(ab1, cd1, 0x20);
    *c = _mm256_permute2f128_pd(ab0, cd0, 0x31);
    *d = _mm256_permute2f128_pd(ab1, cd1, 0x31);
}

static inline void leaf4x4(double *data, const Field *f, bool inverse) {
    Vec a = _mm256_loadu_pd(data), b = _mm256_loadu_pd(data + 4);
    Vec c = _mm256_loadu_pd(data + 8), d = _mm256_loadu_pd(data + 12);
    transpose4(&a, &b, &c, &d);
    Vec p = _mm256_set1_pd(f->p), r = _mm256_set1_pd(f->reciprocal);
    Vec imag = _mm256_set1_pd(inverse ? f->p - f->imag : f->imag);
    if (inverse) {
        Vec ab = add(a, b, p), cd = add(c, d, p);
        Vec ba = sub(a, b, p), dc = mul(sub(c, d, p), imag, p, r);
        a = add(ab, cd, p);
        b = add(ba, dc, p);
        c = sub(ab, cd, p);
        d = sub(ba, dc, p);
    } else {
        Vec ac = add(a, c, p), bd = add(b, d, p);
        Vec ca = sub(a, c, p), db = mul(sub(b, d, p), imag, p, r);
        a = add(ac, bd, p);
        b = sub(ac, bd, p);
        c = add(ca, db, p);
        d = sub(ca, db, p);
    }
    transpose4(&a, &b, &c, &d);
    _mm256_storeu_pd(data, a);
    _mm256_storeu_pd(data + 4, b);
    _mm256_storeu_pd(data + 8, c);
    _mm256_storeu_pd(data + 12, d);
}

// Recursing into contiguous quarters keeps late forward and early inverse
// butterflies within the same small region; each sweep fuses two stages.
static void blocked_transform(double *data, size_t n, const Field *f, bool inverse) {
    if (n < 16) {
        small_transform(data, n, f, inverse);
        return;
    }
    size_t h = n / 4;
    if (!inverse)
        fused_stage(data, h, f, false);
    if (h <= 8) {
        if (h == 8 && !inverse)
            for (size_t j = 0; j < 4; j++)
                pair_stage(data + 8 * j, 4, f, false);
        leaf4x4(data, f, inverse);
        if (h == 8)
            leaf4x4(data + 16, f, inverse);
        if (h == 8 && inverse)
            for (size_t j = 0; j < 4; j++)
                pair_stage(data + 8 * j, 4, f, true);
    } else {
        for (size_t j = 0; j < 4; j++)
            blocked_transform(data + j * h, h, f, inverse);
    }
    if (inverse)
        fused_stage(data, h, f, true);
}

// Same decimation-in-frequency order as the blocked transform. Each cut
// is on a vector boundary and complete subtrees use the radix-4 kernel.
static void forward_prefix(double *data, size_t n, size_t inputs, size_t count,
                           const Field *f) {
    if (count == n) {
        blocked_transform(data, n, f, false);
        return;
    }
    size_t h = n / 2;
    size_t present = inputs < h ? inputs : h;
    size_t paired = inputs - present;
    Vec p = _mm256_set1_pd(f->p), r = _mm256_set1_pd(f->reciprocal);
    double *right = data + h;
    const double *roots = f->forward + h - 1;
    for (size_t i = 0; i < paired; i += 4) {
        Vec a = _mm256_loadu_pd(data + i), b = _mm256_loadu_pd(right + i);
        _mm256_storeu_pd(data + i, add(a, b, p));
        if (count > h)
            _mm256_storeu_pd(right + i, mul(sub(a, b, p), _mm256_loadu_pd(roots + i), p, r));
    }
    if (count <= h) {
        forward_prefix(data, h, present, count, f);
        return;
    }
    for (size_t i = paired; i < present; i += 4)
        _mm256_storeu_pd(right + i, mul(_mm256_loadu_pd(data + i), _mm256_loadu_pd(roots + i), p, r));
    blocked_transform(data, h, f, false);
    forward_prefix(right, h, present, count - h, f);
}

// Beyond count are known time-domain coefficients scaled by n. The
// initial tail is zero. Each recursion derives its own known tail.
static void inverse_prefix(double *data, size_t n, size_t count, const Field *f) {
    if (count == n) {
        blocked_transform(data, n, f, true);
        return;
    }
    size_t h = n / 2;
    double *right = data + h;
    Vec p = _mm256_set1_pd(f->p), r = _mm256_set1_pd(f->reciprocal);
    if (count <= h) {
        Vec half = _mm256_set1_pd((f->p + 1) / 2);
        for (size_t i = count; i < h; i += 4) {
            Vec sum = add(_mm256_loadu_pd(data + i), _mm256_loadu_pd(right + i), p);
            _mm256_storeu_pd(data + i, mul(sum, half, p, r));
        }
        inverse_prefix(data, h, count, f);
        for (size_t i = 0; i < count; i += 4) {
            Vec a = _mm256_loadu_pd(data + i);
            _mm256_storeu_pd(data + i, sub(add(a, a, p), _mm256_loadu_pd(right + i), p));
        }
        return;
    }
    size_t tail = count - h;
    blocked_transform(data, h, f, true);
    const double *roots = f->forward + h - 1;
    for (size_t i = tail; i < h; i += 4) {
        Vec a = _mm256_loadu_pd(data + i);
        Vec diff = sub(a, _mm256_loadu_pd(right + i), p);
        _mm256_storeu_pd(data + i, add(a, diff, p));
        _mm256_storeu_pd(right + i, mul(diff, _mm256_loadu_pd(roots + i), p, r));
    }
    inverse_prefix(right, h, tail, f);
    const double *inverse = f->inverse + h - 1;
    for (size_t i = 0; i < tail; i += 4) {
        Vec a = _mm256_loadu_pd(data + i);
        Vec b = mul(_mm256_loadu_pd(right + i), _mm256_loadu_pd(inverse + i), p, r);
        _mm256_storeu_pd(data + i, add(a, b, p));
        _mm256_storeu_pd(right + i, sub(a, b, p));
    }
}

static void flat_transform(double *data, size_t n, const Field *f, bool inverse) {
    for (size_t h = inverse ? 1 : n / 2; h && h < n; h = inverse ? 2 * h : h / 2) {
        for (size_t start = 0; start < n; start += 2 * h) {
            if (h >= 4)
                pair_stage(data + start, h, f, inverse);
            else
                scalar_stage(data + start, h, f, inverse);
        }
    }
}

static uint64_t shifted_word(const uint64_t *words, size_t len, size_t bit) {
    size_t i = bit / 64;
    unsigned shift = bit % 64;
    if (i >= len)
        return 0;
    uint64_t low = words[i] >> shift;
    if (shift && i + 1 < len)
        low |= words[i + 1] << (64 - shift);
    return low;
}

static inline double reduce_word(uint64_t word, const Field *f) {
    // Split into exactly representable 32-bit chunks.
    double r = mul_scalar((double)(word >> 32), 4294967296.0, f) + (uint32_t)word;
    return r >= f->p ? r - f->p : r;
}

static void pack(const uint64_t *words, size_t len, double *row, size_t count,
                 const Profile *profile, const Field *f) {
    const unsigned bits = profile->bits;
    // Each candidate packs 65–192 bits. One or two high words
    // are combined in Horner form using 2^64 modulo p.
    unsigned top_bits = (bits - 1) % 64 + 1;
    uint64_t mask = UINT64_MAX >> (64 - top_bits);
    size_t top = (bits - 1) / 64;
    for (size_t i = 0; i < count; i++) {
        size_t bit = i * bits;
        double r = reduce_word(shifted_word(words, len, bit + 64 * top) & mask, f);
        for (size_t j = top; j; j--) {
            r = mul_scalar(r, f->radix64, f) +
                reduce_word(shifted_word(words, len, bit + 64 * (j - 1)), f);
            if (r >= f->p)
                r -= f->p;
        }
        row[i] = r;
    }
    memset(row + count, 0, (profile->n - count) * sizeof(*row));
}

static size_t round_up(size_t size, size_t block) {
    return (size + block - 1) & ~(block - 1);
}

static void pack_rows(const uint64_t *words, size_t len, double *rows, size_t count,
                      const Plan *plan) {
    const Profile *profile = &plan->profile;
    const unsigned bits = profile->bits;
    const uint64_t low_mask = (UINT64_C(1) << 48) - 1;
    const unsigned middle_bits = bits < 96 ? bits - 48 : 48;
    const uint64_t middle_mask = (UINT64_C(1) << middle_bits) - 1;
    Vec p[MAX_PRIMES], reciprocal[MAX_PRIMES], radix96[MAX_PRIMES];
    for (unsigned j = 0; j < profile->count; j++) {
        p[j] = _mm256_set1_pd(plan->fields[j].p);
        reciprocal[j] = _mm256_set1_pd(plan->fields[j].reciprocal);
        radix96[j] = _mm256_set1_pd(plan->fields[j].radix96);
    }
    // Extract each coefficient once. Chunks of at most 48 bits convert
    // exactly and are smaller than every prime. Each vector holds four
    // consecutive coefficients so it can be stored in the prime's row.
    for (size_t i = 0; i < count; i += 4) {
        double lows[4] = {0}, middles[4] = {0}, highs[4] = {0};
        for (size_t j = 0; j < 4 && i + j < count; j++) {
            size_t bit = (i + j) * bits;
            lows[j] = shifted_word(words, len, bit) & low_mask;
            middles[j] = shifted_word(words, len, bit + 48) & middle_mask;
            if (bits > 96)
                highs[j] = shifted_word(words, len, bit + 96) & ((UINT64_C(1) << (bits - 96)) - 1);
        }
        Vec low = _mm256_loadu_pd(lows), mid = _mm256_loadu_pd(middles), high = _mm256_loadu_pd(highs);
        const Vec radix48 = _mm256_set1_pd(0x1p48);
        for (unsigned j = 0; j < profile->count; j++) {
            Vec digit = add(low, mul(mid, radix48, p[j], reciprocal[j]), p[j]);
            if (bits > 96)
                digit = add(digit, mul(high, radix96[j], p[j], reciprocal[j]), p[j]);
            _mm256_storeu_pd(rows + j * profile->n + i, digit);
        }
    }
    size_t padded = round_up(count, 4);
    for (unsigned j = 0; j < profile->count; j++)
        memset(rows + j * profile->n + padded, 0, (profile->n - padded) * sizeof(double));
}

static void recover(Plan *plan, uint64_t *out, size_t words) {
    const Profile *profile = &plan->profile;
    size_t coefficients = profile->na + profile->nb - 1, n = profile->n;
    size_t output_bit = 0;
    uint64_t coefficient[MAX_WORDS + 1] = {0};
    memset(out, 0, words * sizeof(*out));
    for (size_t row = 0; row < coefficients; row += 4) {
        Vec digits[MAX_PRIMES];
        double unpacked[MAX_PRIMES][4];
        for (unsigned i = 0; i < profile->count; i++) {
            const Field *f = &plan->fields[i];
            Vec p = _mm256_set1_pd(f->p), r = _mm256_set1_pd(f->reciprocal);
            Vec digit = _mm256_loadu_pd(plan->residues + i * n + row);
            for (unsigned j = 0; j < i; j++)
                digit = mul(sub(digit, wrap_upper(digits[j], p), p),
                            _mm256_set1_pd(plan->inverse[i][j]), p, r);
            digits[i] = digit;
            _mm256_storeu_pd(unpacked[i], digit);
        }
        for (size_t j = 0; j < 4 && row + j < coefficients; j++) {
            uint64_t value[MAX_WORDS] = {0};
            size_t used = 1;
            for (unsigned i = profile->count; i > 0; i--) {
                uint64_t carry = unpacked[i - 1][j];
                for (size_t k = 0; k < used; k++) {
                    u128 next = (u128)value[k] * primes[i - 1].modulus + carry;
                    value[k] = next;
                    carry = next >> 64;
                }
                if (carry)
                    value[used++] = carry;
            }
            uint64_t carry = 0;
            for (size_t k = 0; k < MAX_WORDS; k++) {
                u128 next = (u128)coefficient[k] + value[k] + carry;
                coefficient[k] = next;
                carry = next >> 64;
            }
            assert(!carry);
            unsigned available = profile->bits;
            for (size_t k = 0; available && output_bit < 64 * words; k++) {
                unsigned take = available > 64 ? 64 : available;
                unsigned shift = output_bit % 64;
                size_t dest = output_bit / 64;
                uint64_t low = coefficient[k] & (UINT64_MAX >> (64 - take));
                out[dest] |= low << shift;
                if (shift && dest + 1 < words)
                    out[dest + 1] |= low >> (64 - shift);
                output_bit += take;
                available -= take;
            }
            size_t skip = profile->bits / 64;
            unsigned shift = profile->bits % 64;
            for (size_t k = 0; k < MAX_WORDS - skip; k++)
                coefficient[k] = (coefficient[k + skip] >> shift) |
                                 (shift ? coefficient[k + skip + 1] << (64 - shift) : 0);
            memset(coefficient + MAX_WORDS - skip, 0, skip * sizeof(uint64_t));
        }
    }
    // The final carry may fill the last few bits of the exact product.
    size_t k = 0;
    while (output_bit < 64 * words) {
        unsigned shift = output_bit % 64;
        size_t dest = output_bit / 64;
        out[dest] |= coefficient[k] << shift;
        if (shift && dest + 1 < words)
            out[dest + 1] |= coefficient[k] >> (64 - shift);
        output_bit += 64;
        k++;
    }
}

static void product(Plan *plan, const uint64_t *a, size_t na, const uint64_t *b, size_t nb,
                    uint64_t *output, Mode mode) {
    const Profile *profile = &plan->profile;
    size_t n = profile->n;
    size_t count = mode == TRUNCATED && n >= TRUNCATION_BLOCK
                       ? round_up(profile->na + profile->nb - 1, TRUNCATION_BLOCK) : n;
    void (*transform)(double *, size_t, const Field *, bool) =
        mode == FLAT ? flat_transform : blocked_transform;
    if (mode >= PACKED) {
        pack_rows(a, na, plan->residues, profile->na, plan);
        pack_rows(b, nb, plan->right, profile->nb, plan);
    }
    for (unsigned i = 0; i < profile->count; i++) {
        const Field *f = &plan->fields[i];
        double *left = plan->residues + i * n;
        double *right = plan->right + (mode >= PACKED ? i * n : 0);
        if (mode < PACKED) {
            pack(a, na, left, profile->na, profile, f);
            pack(b, nb, right, profile->nb, profile, f);
        }
        if (count < n) {
            forward_prefix(left, n, round_up(profile->na, 4), count, f);
            forward_prefix(right, n, round_up(profile->nb, 4), count, f);
        } else {
            transform(left, n, f, false);
            transform(right, n, f, false);
        }
        Vec p = _mm256_set1_pd(f->p), r = _mm256_set1_pd(f->reciprocal);
        Vec normalization = _mm256_set1_pd(f->inv_length);
        for (size_t j = 0; j < count; j += 4) {
            Vec b = mul(_mm256_loadu_pd(right + j), normalization, p, r);
            _mm256_storeu_pd(left + j, mul(_mm256_loadu_pd(left + j), b, p, r));
        }
        if (count < n) {
            memset(left + count, 0, (n - count) * sizeof(double));
            inverse_prefix(left, n, count, f);
        } else {
            transform(left, n, f, true);
        }
    }
    recover(plan, output, na + nb);
}

static uint64_t now_ns(void) {
    struct timespec value;
    clock_gettime(CLOCK_MONOTONIC, &value);
    return UINT64_C(1000000000) * value.tv_sec + value.tv_nsec;
}

static uint64_t *read_numbers(const char *path, size_t *len) {
    FILE *file = fopen(path, "r");
    assert(file);
    assert(!fseek(file, 0, SEEK_END));
    long size = ftell(file);
    assert(size > 0 && !fseek(file, 0, SEEK_SET));
    char *text = allocate((size_t)size + 1, 1);
    assert(fread(text, 1, size, file) == (size_t)size);
    fclose(file);
    *len = ((size_t)size + 15) / 16;
    uint64_t *result = allocate(*len, sizeof(uint64_t));
    for (size_t i = 0; i < *len; i++) {
        long start = size > 16 ? size - 16 : 0;
        result[i] = strtoull(text + start, NULL, 16);
        text[start] = 0;
        size = start;
    }
    free(text);
    return result;
}

int main(int argc, char **argv) {
    assert(argc == 5 || argc == 6);
    const char *modes[] = {"flat", "blocked", "packed", "truncated"};
    Mode mode = FLAT;
    while (mode <= TRUNCATED && strcmp(argv[1], modes[mode]))
        mode++;
    assert(mode <= TRUNCATED);
    size_t na, nb;
    uint64_t *a = read_numbers(argv[2], &na), *b = read_numbers(argv[3], &nb);
    unsigned repeats = strtoul(argv[4], NULL, 10);
    assert(repeats);
    uint64_t *output = allocate(na + nb, sizeof(uint64_t));
    uint64_t now = now_ns();
    Plan plan = make_plan(na, nb, mode);
    uint64_t setup = now_ns() - now;
    now = now_ns();
    for (unsigned j = 0; j < repeats; j++)
        product(&plan, a, na, b, nb, output, mode);
    uint64_t elapsed = now_ns() - now;
    size_t significant = na + nb;
    while (significant && !output[significant - 1])
        significant--;
    if (argc == 6) {
        if (!significant)
            puts("0");
        else {
            printf("%lx", output[--significant]);
            while (significant)
                printf("%016lx", output[--significant]);
            puts("");
        }
    } else {
        unsigned bits = significant
                            ? 64 * (significant - 1) + 64 - __builtin_clzll(output[significant - 1])
                            : 0;
        printf("%lu %u\n", elapsed, bits * repeats);
        fprintf(stderr, "%u primes x %u bits, n=%zu, plan=%lu ns\n", plan.profile.count,
                plan.profile.bits, plan.profile.n, setup);
    }
    free_plan(&plan);
    free(output);
    free(a);
    free(b);
}
