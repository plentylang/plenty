# Truncated exact transforms and Newton reciprocals

## Reusing a transform across blocks

The skewed multiplication path divides the long operand into blocks the
size of the short operand. Previously each block prepared a fresh transform
of both inputs. The short operand, transform length, and roots are common
to all full blocks. Preparing that operand once reduces the number of
transforms per prime for `t` full blocks from `3t` to `2t + 1`. The root
table is also built once per prime. The five inverse rows and a limb row
are shared by all blocks. The partial products add into the output at
their block offsets. The short tail uses ordinary multiplication.

For transform length `N` and block width `w` limbs, the prepared inputs
and roots occupy `5(N + N/2)` words. The inverse rows and partial product
use another `5N + 2w` words. Before reuse the corresponding upper bound
was `2N + N/2 + 4(w+1) + 2w` words, with the transform buffers allocated
again for each block.

## Truncated linear convolution

The packed convolution still uses five independently certified primes.
For inputs of `a` and `b` limbs, the product needs `ceil((a+b)/2)` packed
coefficient slots, including the output pair that may hold only carry.
The forward transform computes that prefix of the bit-reversed evaluation
order. Each decimation step splits the sums and differences of the input
halves. The transform skips branches with no required values and
multiplications on zero input pairs.

The inverse uses the complementary truncated transform. Inside a recursive
subproblem, positions beyond the requested values hold known coefficients
scaled by the local transform length. At the root these coefficients are
all zero. The known positions let a completed half reconstruct the input
to the second half. Full subtransforms use the existing iterative kernels
with the appropriate stride in the root table. See
[van der Hoeven's truncated FFT](https://www.texmacs.org/joris/issac04/issac04.html)
and the [FLINT algorithm description](https://flintlib.org/doc/algorithms/fft/truncated-fft.html).
Cyclic correction products use the full transform.

In the ordinary Montgomery representation, a loaded coefficient has
factor `R = 2^64`. For a nonsquare product, one operand loads with factor
`1/N`, which puts the pointwise product in standard representation at
scale `1/N`. The inverse therefore needs no normalization pass. A square
applies this adjustment in the pointwise loop. Since `N` divides `p-1`,
the scale `1/N mod p` is `p - (p-1)/N`.

## Newton reciprocal

Let `B = 2^64` and `D_m` be the leading `m` limbs of the normalized
divisor rounded up by one. The fixed-point reciprocal at that precision is
`I_m = B^(2m)/D_m`. The seed uses the existing word division to obtain a
lower estimate within two units. If rounding the prefix overflows its
width, the rounded divisor is exactly `B^m` and the reciprocal is
`B^m`.

Each step increases the precision from `l` to `m <= 2l-1`. If the old
estimate `T` is below `I_l`, the lifted value `X = T*B^(m-l)` is below
`I_m` because the normalized rounded prefix decreases as more limbs are
included. The inverse of a value in `[1/2, 1]` changes by at most four
times the change in that value. Together with the two-unit error on
`T`, this bounds the error on `X` by `6*B^(m-l)`.

Write `E = B^(m+l) - D_m*T`. It is nonnegative and smaller than
`6*B^m`, so subtracting the low `m+1` limbs of `D_m*T` from zero
recovers it exactly. The Newton correction to `X` is
`floor(T*E/B^(2l))`. Its real-valued update squares the relative error.
After rounding down, the absolute error on the new estimate is below
`1 + 36*B^(m-2l) < 2`. After the final step, dropping one guard limb
and accounting for the rounded divisor prefix gives a lower estimate
within two units for the original divisor. The existing exact Barrett
correction therefore retains its bound.

For rendering, the Newton constructor is used at widths of at least
4,096 reciprocal limbs. Smaller cached reciprocals retain division-based
construction. General division uses Newton to prepare once per call when
the normalized divisor has at least 6,144 limbs, the quotient is between
one and two times the divisor's limb count, the cyclic modulus fits within
four-thirds of the divisor width, and every block can use the shortened
correction. The power-of-two cyclic length still leaves some shapes on
recursive division.

The pinned Malachite 0.14.0 source uses Newton approximation and a smaller
cyclic correction modulus, as described in
[the earlier division experiment](../block-division/README.md). Plenty
retains its five-prime integer transform and uses the same multiplication
dispatcher to construct the reciprocal.

## End-to-end timing

Median milliseconds per operation on Linux x86-64, Ryzen 7 7840HS, pinned
to CPU 0 with seven interleaved batches after warmup. The bit count refers
to the longer multiplier, the divisor, or the value converted. A balanced
division has a quotient about as wide as its divisor.

| Workload | Bits | Prior Plenty | This change | Malachite 0.14.0 |
| --- | ---: | ---: | ---: | ---: |
| Dense multiply | 131,136 | 13.30 | 6.14 | 0.45 |
| Division, double-width quotient | 393,216 | 81.53 | 54.84 | 5.08 |
| Division, one-third-width quotient | 524,288 | 44.42 | 15.71 | 1.89 |
| Balanced division | 786,432 | 133.73 | 85.41 | 13.89 |
| Decimal rendering | 786,432 | 108.56 | 87.31 | 12.15 |
| Multiply, 16:1 ratio | 1,048,576 | 11.14 | 7.62 | 1.17 |
| Balanced division | 1,048,576 | 72.62 | 81.42 | 7.49 |
| Balanced division | 1,572,864 | 140.79 | 87.89 | 10.61 |
| Decimal rendering | 1,572,864 | 389.58 | 296.21 | 30.07 |

A second nine-batch sweep confirmed the leading gains. At 524,288
divisor bits and a one-third-width quotient, division fell from 35.01
to 12.43 ms. At 1,572,864 bits, balanced division fell from 233.59 to
136.70 ms. The 16:1 multiply went from 15.73 to 9.47 ms.

The host was busy. Repeat runs varied enough that small changes in
elapsed time are inconclusive. At 65,536 and 524,288 bits, the
power-of-two dense multiplication controls differed by less than one
percent in process CPU time, despite wider elapsed-time variation.
Balanced division with a 1,048,576-bit divisor stays on the recursive path
and is a control for the cyclic cutoff. A second set of inputs pinned to
CPU 3 reversed the direction of the elapsed change, from 133.69 to
110.28 ms. These runs leave the latency change in the control inconclusive.

In a separate 3:1 multiplication with a 3,145,728-bit longer operand,
maximum process RSS increased from 5,236 to 5,884 KiB. The prepared
transforms trade memory for elapsed time. In a 16:1 multiplication
counter run with a 1,048,576-bit longer operand, instructions fell from
3.02 to 1.86 billion and cycles from 999 to 630 million across 12 products
and process overhead.

## Isolating the changes

The first timings used standalone copies of the same module and the same
optimized compiler. The copies bypassed standard-library-only diagnostic
traps. Every variant produced the same checksums. At 1,048,576 bits in the
long operand and a 16:1 ratio, retaining the short transform alone cut
multiplication from 11.45 to 7.26 ms. At 524,288 divisor bits and a
one-third-width quotient, adding truncation to the reuse variant cut
division from 33.26 to 11.72 ms because the correction product could use
blocked transforms at its operand sizes.

The reciprocal constructor benchmark timed only construction, with
identical multiplication code on both sides. These are medians of seven
interleaved batches with the leading divisor word normalized.

| Reciprocal limbs | Division-based, ms | Newton, ms |
| ---: | ---: | ---: |
| 768 | 1.77 | 4.45 |
| 2,048 | 7.10 | 8.37 |
| 4,096 | 23.03 | 22.94 |
| 6,144 | 44.16 | 36.77 |
| 8,192 | 47.93 | 32.25 |
| 16,384 | 123.03 | 73.41 |

### Reproduction

Build the benchmark in each revision with that revision's optimized
compiler, then pass both executables and the Malachite comparator to
the same runner. The baseline was `7ea170da`.

```sh
RUSTC_WRAPPER= cargo build --locked --release --bin plenty
target/release/plenty --compile experiments/bigint-comparison/bench.plenty \
  --no-cache --no-strip -o /tmp/plenty-revision-bench
RUSTC_WRAPPER= cargo build --locked --release \
  --manifest-path experiments/bigint-comparison/Cargo.toml \
  --target-dir target/bigint-comparison/rust-target
python3 -B experiments/bigint-comparison/block-division/measure.py /tmp/truncated.csv \
  main=/tmp/plenty-baseline-bench candidate=/tmp/plenty-revision-bench \
  malachite=target/bigint-comparison/rust-target/release/plenty-bigint-comparison \
  --cpu 0 --shape-sweep \
  --bits 65536 65600 131136 196608 262144 262400 393216 524288 786432 1048576 1572864 \
  --cases div/balanced div/short div/quotient-1-3 mul/dense mul/ratio-3 mul/ratio-16 parse/decimal render/decimal
```

Use `--bits 393216 524288 786432 --cases div/quotient-2 div/quotient-3`
to cover both wider quotients. The runner skips wider quotient fixtures
whose operands would exceed the hex input limit. It checks the result checksum
for every engine. The pinned library was used as a second implementation
for the [existing differential corpus](../README.md#reproduce).

## Correctness checks

The optimized compiler's standard-library build agreed with Python
integers on 240 full products and 308 signed quotient and remainder pairs.
The cases include carry chains, widths around the transform crossover,
skewed products with tails, and divisions with omitted low zero limbs.
A standalone build exposing the private Newton constructor passed 240
checks of its two-unit error bound, including overflowing and nearly
half-base prefixes. The Malachite corpus matched all 16,378 output lines
over 195 cases, and the five-prime constant and carry checks passed.
