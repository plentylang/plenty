# Fractional radix digits, two-block Barrett division, and vector multiplication

## Production algorithms

A non-power-of-two radix chunk uses `D = b^k < B = 2^64`, where
`b` is the radix and `k` is the number of digits that fit in one limb.
Short division already returns a remainder `r < D`. One more quotient
step with the same normalized divisor and reciprocal computes
`f = floor(Br/D) + 1`. Because `D < B`, the rounded fraction obeys
`r/D < f/B < (r+1)/D`. Multiplying by `b` yields the next digit in the
high word and the next fractional remainder in the low word. After `k`
steps the emitted digits are the exact representation of `r`, including
leading zeroes within each chunk. The final ungrouped word uses scalar
division. This is the fractional-limb method described in the
[GMP manual](https://gmplib.org/manual/Binary-to-Radix.html) and used by
Malachite 0.14.0. A decimal-specific trial with two wide multiplies and a
60-bit fraction had mixed timings. The shipped loop is shared across radices.

Malachite can use two blocks when a Barrett quotient is close to the
divisor width. Plenty now considers this plan for quotients at least
about half as wide as the significant divisor and below its width.
The existing cyclic-width and minimum-block checks still apply.
For an odd width, Algorithm D handles the leading limb while handling
the initial quotient digit. The two remaining equal blocks can then
reuse the reciprocal transform as well as the divisor transform.
Removing one limb can disqualify the two-block plan when the smaller
block no longer fits the correction modulus. Reciprocal construction
and transform preparation are included in the division timings.

## Production timings

These are end-to-end medians from separate optimized baseline and candidate
compilers. The division run used nine pinned, interleaved batches and the
rendering run used eleven. Both used core 12 on the Ryzen 7 7840HS and
compared against Malachite 0.14.0.

| Divisor bits | Quotient shape | Baseline | Candidate | Malachite |
| ---: | --- | ---: | ---: | ---: |
| 524,288 | One limb shorter | 38.43 ms | 28.30 ms | 3.57 ms |
| 524,352 | One limb shorter | 40.68 ms | 25.83 ms | 3.32 ms |
| 589,888 | One limb shorter, two-block plan ineligible | 62.62 ms | 64.27 ms | 4.83 ms |
| 786,432 | Balanced | 67.24 ms | 67.05 ms | 7.83 ms |
| 1,048,576 | One limb shorter | 112.14 ms | 69.89 ms | 11.51 ms |
| 1,048,576 | Three-quarters width | 95.36 ms | 71.50 ms | 9.21 ms |
| 1,572,864 | One limb shorter | 221.00 ms | 129.90 ms | 16.63 ms |

| Input bits | Radix | Baseline | Candidate | Malachite |
| ---: | ---: | ---: | ---: | ---: |
| 4,096 | 10 | 0.0953 ms | 0.0901 ms | 0.0081 ms |
| 4,096 | 36 | 0.0668 ms | 0.0601 ms | 0.0059 ms |
| 16,384 | 36 | 0.4627 ms | 0.4918 ms | 0.0508 ms |
| 65,536 | 10 | 3.2868 ms | 3.0564 ms | 0.5051 ms |
| 262,144 | 10 | 26.6524 ms | 27.0515 ms | 3.8462 ms |
| 262,144 | 36 | 16.4113 ms | 14.7226 ms | 2.6772 ms |

The largest repeatable gains came from near-balanced division. Rendering
results varied with width and radix, and shared host activity raised timing
variance in the first sweeps. The independent Python oracle compared 176
signed divisions and 288 complete signed radix strings for exact equality.

## Vector experiment

`vector-product.c` is a standalone unsigned multiplication prototype
inspired by Malachite's FLINT-derived small-prime path. Both modes choose
a profile of four to six primes above `2^49`, with packed
coefficients of 80–140 bits. A candidate with `s` primes and `w`-bit
coefficients qualifies when
`2w + ceil(log2(min(na, nb))) <= 49s`. Here `na` and `nb` are the
input coefficient counts. The CRT modulus exceeds every integer
convolution coefficient. The selector compares `s * N * log2(N)` for
power-of-two transform length `N`.

Each prime is below `2^50`. The FMA modular product uses round-to-nearest
for the approximate quotient and two exact differences to recover the
integer remainder. The bound for canonical residues is described in the
[previous single-prime experiment](../barrett-plans/README.md#packing-and-vector-experiments).
The prototype packs the operands, transforms each prime row, multiplies
pointwise, inverts, performs four coefficients of mixed-radix CRT in
parallel, and carries the recovered coefficients into the output limbs.

The flat mode uses radix-2 stages. The blocked mode fuses two stages,
recurses on contiguous quarters, and transposes four small butterflies
so the leaves can also use SIMD. Both use full power-of-two transforms.
Malachite uses truncated transforms and static twiddle tables. The
prototype generates a plan per new operand shape and reports that setup
separately. This benchmark is limited to unsigned Linux x86-64 with AVX2
and FMA. `std.bigint` continues to use its Plenty transform.
[FLINT small-prime transforms](https://flintlib.org/doc/algorithms/fft/small-prime-fft.html)

## Full-width multiplication timings

The table uses medians from nine pinned, randomized batches on a
Ryzen 7 7840HS. The C measurements include packing, transforms,
pointwise multiplication, CRT and carry reconstruction. The plan
column separately measures allocations and twiddle generation for a
new operand shape. The baseline is Plenty at `ffce9762`. The
comparison engine is Malachite 0.14.0 built for `x86-64-v3`.

| Longer operand bits | Ratio | Plenty | Flat AVX2 | Blocked AVX2 | Plan | Malachite |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1,048,576 | 1:1 | 26.95 ms | 12.64 ms | 7.67 ms | 1.28 ms | 3.89 ms |
| 1,048,576 | 3:1 | 26.14 ms | 6.59 ms | 3.60 ms | 0.96 ms | 3.16 ms |
| 1,572,864 | 1:1 | 42.64 ms | 18.64 ms | 8.08 ms | 2.04 ms | 5.72 ms |
| 3,145,728 | 1:1 | 105.88 ms | 37.55 ms | 19.49 ms | 4.13 ms | 15.35 ms |

Using four vector lanes to generate the roots lowered plan time from
1.54 to 1.29 ms at 1,048,576 bits in a separate 11-batch paired run.
Both vector modes matched the full Python product for 192 seeded
cases with dense, sparse, maximum-limb and unequal operands. The same
192 cases passed with Clang's undefined-behavior traps enabled.

## Reproduction

Build an optimized Plenty compiler from both revisions. With each one,
compile `experiments/bigint-comparison/bench.plenty` using
`--compile --no-cache --no-strip`. Build the Malachite 0.14.0 comparator
from the benchmark workspace lockfile for `x86-64-v3`.

Run the existing benchmark harness with all paths resolved locally.

```sh
python3 -B experiments/bigint-comparison/block-division/measure.py \
  division.csv base=baseline-bench candidate=candidate-bench \
  malachite=plenty-bigint-comparison --cpu 10 --batches 9 --shape-sweep \
  --bits 393216 524224 524288 524352 589824 589888 786432 1048576 \
         1048640 1179648 1572864 \
  --cases div/balanced div/quotient-one-limb-shorter \
          div/quotient-three-quarters div/quotient-1-2 div/quotient-one-limb-wider
python3 -B experiments/bigint-comparison/block-division/measure.py \
  render.csv base=baseline-bench candidate=candidate-bench \
  malachite=plenty-bigint-comparison --cpu 10 --batches 9 \
  --bits 64 256 1024 4096 6144 8192 16384 65536 262144 1048576 \
  --cases render/decimal render/base36
cc -std=gnu17 -O3 -mavx2 -mfma -ffp-contract=off -Wall -Wextra -Werror \
  experiments/bigint-comparison/radix-vector/vector-product.c -o vector-product
python3 -B experiments/bigint-comparison/radix-vector/measure-vector.py \
  vector.csv --plenty ./baseline-bench --malachite ./plenty-bigint-comparison \
  --vector ./vector-product --cpu 10 --batches 9
```

The drivers check timed results and interleave executables after one warmup
batch. The vector driver checks each full product against Python before
timing it. Raw measurements and the extended independent oracles are in
`target/issue-987-radix-vector/` in the primary checkout.
