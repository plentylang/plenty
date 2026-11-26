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
inspired by Malachite's FLINT-derived small-prime path. All modes choose
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
The packed mode splits each coefficient into at most three chunks of up to
48 bits. Each chunk converts exactly to a double and fits below every
prime. The mode loads four consecutive coefficients once and reduces them
into each prime row with AVX2/FMA. It holds both operands' prime rows
during packing.

The truncated mode uses the same packing and rounds the number of
required convolution points up to a multiple of 256. It skips unneeded
butterflies in both directions and uses the blocked kernel for complete
subtrees. The inverse receives zero coefficients beyond the prefix,
as those slots are outside the actual product. Malachite uses truncated
transforms and static twiddle tables. The
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
The original flat and blocked modes matched 192 full Python products.

## Shared packing and truncation timings

These medians use nine interleaved batches on CPU 12 of the Ryzen 7
7840HS, with twelve products per sample. Plenty is the optimized baseline
at `cde032589` and Malachite is 0.14.0 built for `x86-64-v3`. Times are in
milliseconds. The last column is the truncated prototype's plan setup
once per shape. The other vector columns time multiplication with a
prepared plan, including packing, CRT and carry recovery.

| Longer operand bits | Ratio | Plenty | Blocked | Packed | Truncated | Malachite | Plan |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 720,960 | 1:1 | 10.898 | 3.166 | 1.455 | 1.235 | 1.556 | 0.693 |
| 720,960 | 3:1 | 7.731 | 2.549 | 0.829 | 0.776 | 0.969 | 0.434 |
| 1,048,576 | 1:1 | 15.844 | 5.893 | 1.995 | 1.970 | 1.688 | 0.872 |
| 1,048,576 | 3:1 | 23.468 | 3.400 | 2.005 | 1.670 | 2.546 | 0.760 |
| 3,145,728 | 1:1 | 49.800 | 12.718 | 6.503 | 5.785 | 6.466 | 2.625 |
| 3,145,728 | 3:1 | 35.275 | 11.326 | 3.814 | 3.679 | 3.973 | 1.474 |

For equal operands, 720,960 bits fills 76.6% of the padded transform
after rounding and truncation saves 15.1% of packed product time.
At 1,048,576 bits, occupancy is 92.2% and the saving is 1.2%.
Adding the separate plan and prepared-product medians gives 1.928 ms
and 2.842 ms respectively. Target dispatch and measured one-shot latency
remain open questions for a production vector kernel.

All four modes matched 416 exact Python products with repeated use of
the plan, including dense, sparse, all-one and unequal operands.
Clang AddressSanitizer and UndefinedBehaviorSanitizer were enabled.
Leak detection was disabled for the sandbox run.

## Rendering workspace trial

The renderer copies the magnitude once and normalizes each dividend in
place. Its remainder occupies the same slice. The quotient is written into
the front of a single workspace, and each child uses the unused suffix
for its own quotients. The largest output width at each depth bounds the
workspace, including one possible leading zero and one spare normalization
limb per level. Whole zero divisor limbs stay in the remainder.

One growable quotient list per depth improved several large cases but added
collection bookkeeping at smaller widths. At 8,192 bits in decimal the
baseline, per-depth buffers, and single workspace took 114.7, 153.1, and
112.6 µs respectively. This was a separate 12-batch run with 2,000
conversions per sample on CPU 14.

The table uses eleven interleaved batches of optimized Plenty binaries
with the candidate embedded in `std.bigint`, plus Malachite 0.14.0, on
CPU 4. Times are in milliseconds and include the power cache and allocation
costs of each conversion.

| Bits | Radix | Baseline | Workspace | Malachite |
| ---: | ---: | ---: | ---: | ---: |
| 65,536 | 10 | 1.911 | 1.691 | 0.349 |
| 65,536 | 3 | 2.904 | 2.650 | 0.498 |
| 65,536 | 36 | 1.677 | 1.475 | 0.297 |
| 262,144 | 10 | 15.305 | 14.163 | 2.842 |
| 262,144 | 3 | 20.566 | 19.112 | 3.190 |
| 262,144 | 36 | 12.872 | 11.700 | 2.240 |
| 1,048,576 | 10 | 102.048 | 96.570 | 13.692 |
| 1,048,576 | 3 | 147.046 | 142.040 | 17.572 |
| 1,048,576 | 36 | 96.756 | 91.070 | 12.865 |

The independent Python oracle compared 288 complete signed strings across
all radices. The Plenty regression also checks an odd digit width with
unequal siblings under an exact cap and a padded cap.

## Cofactor CRT trial

The portable transform trial folded each inverse cofactor into the
existing inverse transform scale. Reconstruction multiplied each weighted
residue by its four-limb cofactor and summed the five terms. The sum is
less than five times the full modulus. Dividing the high limb by the
modulus's high limb estimates the quotient at most one too high, so
the reduction subtracts one multiple of the modulus and adds it back
on borrow.

Two preview implementations used a tuple helper and a fixed array loop.
The table shows selected end-to-end medians in milliseconds from eleven
interleaved batches on CPU 8. Some of the larger products favored the
loop, but other widths and callers regressed. Plenty retains its
Garner reconstruction.

| Operation | Bits | Garner | Cofactor tuple | Cofactor loop |
| --- | ---: | ---: | ---: | ---: |
| Equal multiplication | 262,144 | 10.887 | 11.259 | 10.962 |
| Equal multiplication | 1,048,576 | 15.123 | 15.008 | 14.744 |
| Equal multiplication | 1,572,864 | 25.108 | 24.250 | 23.214 |
| Balanced division | 720,960 | 66.579 | 78.963 | 82.390 |

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
python3 -B experiments/bigint-comparison/block-division/measure.py \
  workspace.csv base=baseline-bench workspace=candidate-bench \
  malachite=plenty-bigint-comparison --cpu 4 --batches 11 \
  --bits 4096 8192 16384 65536 262144 1048576 \
  --cases render/decimal render/ternary render/base36
cc -std=gnu17 -O3 -mavx2 -mfma -ffp-contract=off -Wall -Wextra -Werror \
  experiments/bigint-comparison/radix-vector/vector-product.c -o vector-product
python3 -B experiments/bigint-comparison/radix-vector/measure-vector.py \
  vector.csv --plenty ./baseline-bench --malachite ./plenty-bigint-comparison \
  --vector ./vector-product --cpu 12 --batches 9 --min-count 12 \
  --modes blocked packed truncated --bits 720960 1048576 3145728
python3 -B experiments/bigint-comparison/radix-vector/check-vector.py ./vector-product
```

The drivers check timed results and interleave executables after one warmup
batch. The vector driver checks each full product against Python before
timing it. Raw measurements are in `target/issue-987-radix-vector/`
and `target/issue-987-pack-crt/` in the primary checkout.
