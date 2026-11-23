# Reusing transforms during Barrett division

A Barrett quotient block multiplies the top of the running remainder by
a fixed-point reciprocal to estimate the quotient. It then subtracts
the quotient multiplied by the normalized divisor. The divisor and
reciprocal stay fixed across blocks, so Plenty can prepare a forward
transform of each fixed operand and use one forward transform of the
varying operand plus one inverse transform per prime and block. Each
prepared transform stores its roots and includes the inverse transform's
normalization factor. One set of five inverse rows is reused for both
products, using the larger transform's size. Skewed multiplication
uses the same prepared-product implementation.

The divisor is prepared for the cyclic correction at the chosen power
of two. If the divisor or quotient exceeds the modulus width, its
high limbs are folded into the low row. The correction continues to use
the original limbs for the short low product and reconstructs the exact
difference as described in [cyclic products](../cyclic-products/README.md).
A short final quotient block takes the corresponding suffix of the
reciprocal, so its linear estimate uses ordinary multiplication.
Repeated radix splits cache the transforms with the normalized
divisor and reciprocal within one conversion.

With the prepared operands, the Barrett path can handle quotients
wider than twice the divisor. For a quotient of `h` limbs and divisor
of `n` significant limbs, choose `ceil(h / (n - 1))` blocks, each up
to `ceil(h / blocks)` limbs wide. The last block must satisfy the
existing cyclic-product crossover and minimum tail size. The code
computes the tail from the excess due to rounding, using a remainder
instead of multiplying the block count by its width. The scratch size
for the transforms depends on `n` even as `h` increases.

## Elapsed time

Optimized programs ran on the Ryzen 7 7840HS. The bench harness measures
division and rendering operations with setup included. It reads operands
before starting the timer. Each median uses seven batches after warmup
with randomized interleaving on CPU 10. The baseline is the bigint
implementation merged at `e99e5ea1` and the comparator is Malachite
0.14.0 built for `x86-64-v3`. All result checksums agreed.

| Divisor or value bits | Workload | Prior Plenty | Prepared | Malachite |
| ---: | --- | ---: | ---: | ---: |
| 393,216 | Triple-width quotient | 72.65 ms | 37.11 ms | 4.62 ms |
| 524,288 | Balanced division | 26.66 ms | 22.99 ms | 2.79 ms |
| 524,352 | Triple-width quotient | 113.58 ms | 53.84 ms | 7.97 ms |
| 528,384 | Triple-width quotient | 106.88 ms | 47.66 ms | 6.90 ms |
| 589,824 | Triple-width quotient | 168.87 ms | 78.31 ms | 14.88 ms |
| 589,888 | Triple-width quotient outside cutoff | 146.35 ms | 156.24 ms | 10.91 ms |
| 786,432 | Triple-width quotient | 281.31 ms | 119.98 ms | 18.49 ms |
| 1,048,576 | Balanced division | 79.67 ms | 74.27 ms | 11.93 ms |
| 1,048,576 | Double-width quotient | 126.18 ms | 114.21 ms | 18.02 ms |
| 1,572,864 | Decimal rendering | 238.62 ms | 236.17 ms | 29.25 ms |
| 3,145,728 | Decimal rendering | 563.28 ms | 535.10 ms | 67.39 ms |
| 3,145,728 | Base-36 rendering | 602.98 ms | 568.86 ms | 72.37 ms |

A follow-up with the test suite idle used eleven interleaved batches
of the two Plenty builds. Triple-width division at 786,432 bits took
164.63 ms before and 69.67 ms after, a 58% reduction. The 589,888-bit
case outside the cyclic cutoff took 110.52 ms before and 110.61 ms
after. Decimal rendering at 1,048,576 bits took 101.29 ms before and
98.91 ms after.

A separate 786,432-bit division with a triple-width quotient measured
peak process RSS at 4,508 KiB before and 6,480 KiB after. In a separate
five-division process on the same operands, userspace instructions fell
from 18.31 to 7.47 billion and cycles fell from 5.57 to 2.25 billion.
The base-case division loop accounted for 24% of the baseline cycle
samples and less than 2% after the fast path widened.
The elapsed-time measurements above include the cost of preparing the
operands and allocating scratch.

## Packing and vector experiments

Malachite's small-prime transform selects a packing profile based on
operand size. A linear-product prototype packs 112 bits per coefficient
with four of Plenty's current primes. The product of the four primes is
greater than `32768 * (2^112 - 1)^2`, so the reconstruction is exact
when the shorter input uses at most 32,768 such coefficients. The first
version lost time when packing increased the power-of-two transform
length. A second version selected the narrower packing only when the
length stayed the same. In seven interleaved batches, a dense product
at 524,352 bits took 7.55 ms versus 8.51 ms, and a 2:1 product with a
524,288-bit longer operand took 5.14 ms versus 5.70 ms.
The four-prime prototype passed 39 exact product checks. A production
version would need shared packing and reconstruction code to avoid
maintaining two implementations. The prototype uses the existing
cyclic-product layout for division. See [FLINT's packing profiles](https://flintlib.org/doc/algorithms/fft/multimodular.html#profiles).

A standalone C prototype computes one exact cyclic convolution modulo
`p = 1125844072267777 = 262131 * 2^32 + 1`, a Proth prime with witness
5. It compares scalar Montgomery arithmetic with a four-lane AVX2/FMA
radix-2 transform, both with precomputed roots and converted inputs.
For canonical residues `a, b < p < 2^50`, the rounded double product
`H`, reciprocal `1/p` and their rounded product perturb the exact
quotient `ab/p` by less than 0.376. Rounding to an integer `q` adds
at most 0.5, so `|ab - q*p| < p`. An FMA recovers `ab - H` and another
forms `H - q*p`. Both differences are integers within the exact double
range. Their sum is the exact signed residue, which needs at most one
addition of `p`.

The prototype's checksums match Montgomery arithmetic in every timed
batch. It also compares every coefficient through 256 points and 16
coefficients at each larger width against direct convolution with
128-bit products. Flat and blocked vector variants improve on the
scalar kernel at 4,096 points, but have mixed timings from 16,384 to
262,144 points. This is a single-prime kernel experiment. Both the
packing and the vector prototypes remain outside the production path.
See [FLINT's floating-point modular transform](https://flintlib.org/doc/algorithms/fft/small-prime-fft.html).

## Reproduction

Build optimized compilers at the baseline and candidate revisions and
compile `experiments/bigint-comparison/bench.plenty` with each using
`--compile --no-cache --no-strip`. Build the Malachite comparator from
the lockfile in `experiments/bigint-comparison`. Run the shared driver:

```sh
python3 -B experiments/bigint-comparison/block-division/measure.py \
  times.csv baseline=baseline-bench prepared=candidate-bench \
  malachite=plenty-bigint-comparison --cpu 10 --batches 7 --shape-sweep \
  --bits 393216 524288 524352 528384 589824 589888 786432 1048576 \
         1048640 1179648 1179712 1572864 \
  --cases div/balanced div/quotient-2 div/quotient-3 render/decimal
```

The public regression constructs numerators from shifts and additions
and checks signed quotients with different final block widths in both
compiler modes. An independent Python oracle checks 212 signed divisions
around the crossover and with varied quotient widths, whole low zero
limbs, and normalization shifts. Twelve full-string checks independently
assemble positive and negative values in bases 3, 10 and 36. Raw timing
batches and the isolated packing and vector prototypes are retained in
`target/issue-987-barrett-plans/` in the primary checkout.
