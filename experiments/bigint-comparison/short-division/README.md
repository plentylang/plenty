# Bigint short-quotient division and skewed products

## Algorithm

Malachite 0.14.0 handles a short quotient by dividing the leading
`2k` dividend limbs by the leading `k` divisor limbs, then multiplying
by the ignored divisor limbs to correct the remainder. The source calls
this path `limbs_div_mod_balanced` because the dividend and divisor
lengths are relatively close. It computes the truncated division with
schoolbook, recursive, or Barrett division according to `k`. The
[pinned source](https://docs.rs/crate/malachite-nz/0.14.0/source/src/natural/arithmetic/div_mod.rs)
and [GMP's small-quotient algorithm](https://gmplib.org/manual/Small-Quotient-Division)
describe the estimate and correction.

Plenty already uses a leading-limb estimate for recursive quotient
blocks. After resolving the top quotient limb, a short quotient has one
block of `k` limbs. Let `B = 2^64` and write the normalized divisor as
`D = H * B^(n-k) + L`. The leading-limb division estimates the quotient
by dividing the dividend prefix by `H`. The trial quotient is no lower
than the exact quotient `q`. If `r` is the exact remainder, the prefix
exceeds `q * H` by `floor((q * L + r) / B^(n-k))`.
Since `q < B^k` and `r < D`, that excess is below `H + B^k`, which
is at most `3H` by normalization. The trial quotient can therefore
exceed `q` by at most two. The existing saturation and add-back logic
gives the exact quotient and remainder.
When `k` is about one quarter of `n`, the correction multiplies a
`k`-limb quotient by about `3k` low divisor limbs, outside the range
of the direct transform.

To handle that product, split the longer operand into blocks the width
of the shorter operand and multiply each block by the shorter operand.
The first product writes directly to the zeroed result. Remaining
products share one scratch row and add into the result at their limb
offsets. The final block can be shorter and goes through ordinary
multiplication dispatch. All partial products are nonnegative, so each
accumulated sum fits in the full product's width. The selected path
requires a ratio greater than 2:1 and a full block that satisfies the
existing packed transform's size and occupancy checks. A short quotient
enters recursive division only when its correction can use the direct
or blocked transform.

## Prototype ablation

The pilot compiled optimized standalone copies with the same compiler
and four settings for the bigint module. An alternate local import
required replacing standard-library-only traps on unreachable benchmark
paths with error returns in all four copies. Production measurements
below use the stdlib embedded in separate release compilers.

| Operation at 1,048,576 bits | Baseline | Blocks only | Estimate only | Both |
| --- | ---: | ---: | ---: | ---: |
| Divmod, quotient 1/4 of divisor | 94.58 | 94.82 | 103.16 | 21.60 |
| Divmod, quotient 1/8 of divisor | 48.48 | 48.24 | 58.37 | 15.69 |
| Divmod, quotient 1/16 of divisor | 25.16 | 24.96 | 31.48 | 13.16 |
| Multiply, 3:1 | 170.48 | 17.43 | 161.90 | 17.96 |

The estimate alone still sends the skewed correction to schoolbook.
Blocking without a transform slowed a 131,072-bit 8:1 product from
1.58 ms to 2.32 ms and a 262,144-bit 16:1 product from 3.04 ms to 4.68 ms.
The production gate keeps these shapes on the prior path.

Another sweep let blocked multiplication compete with recursion for
ratios above 3:2 when the direct transform was ineligible. It reached the
524,288-bit one-third quotient case and took 23.48 ms versus 55.07 ms
with the narrower gate. The wider gate also recorded slower balanced
division at several sizes during that sweep. The production path keeps
the 2:1 boundary. The raw observations are in `gate.csv`.

## Elapsed time

Optimized builds from `67e443be` and this branch were compared with
the locked Malachite 0.14.0 comparator. The runner pinned each process
to CPU 0 on a Ryzen 7 7840HS under Linux x86-64. Each cell is the median
of seven interleaved, warmed batches in milliseconds per operation.
Operand loading and digit preparation are excluded. Checksums agree in
every batch. For division, the bit count describes the divisor. For
multiplication, it describes the longer input. The first two rows use
the repeat after the full test suite finished. The other rows use the
larger sweep in `quiet.csv`.

| Bits | Operation | Baseline (ms) | Candidate (ms) | Malachite (ms) |
| ---: | --- | ---: | ---: | ---: |
| 262,144 | Divmod, quotient 1/4 | 6.33 | 4.08 | 0.46 |
| 262,400 | Divmod, quotient 1/4 | 6.37 | 6.38 | 0.47 |
| 524,288 | Divmod, quotient 1/4 | 24.60 | 9.31 | 0.99 |
| 524,288 | Divmod, quotient 1/3 | 33.83 | 33.32 | 1.44 |
| 1,048,576 | Divmod, equal-length quotient | 71.46 | 70.84 | 7.71 |
| 1,048,576 | Divmod, quotient 1/2 | 37.17 | 37.37 | 3.94 |
| 1,048,576 | Divmod, quotient 1/3 | 128.15 | 28.48 | 3.32 |
| 1,048,576 | Divmod, quotient 1/4 | 95.95 | 22.08 | 2.46 |
| 1,048,576 | Divmod, quotient 1/8 | 47.80 | 15.49 | 1.84 |
| 1,048,576 | Divmod, quotient 1/16 | 24.88 | 13.00 | 1.50 |
| 1,048,576 | Multiply, equal lengths | 14.38 | 14.72 | 2.22 |
| 1,048,576 | Multiply, 3:1 | 165.30 | 16.90 | 1.72 |
| 1,048,576 | Multiply, 16:1 | 31.85 | 11.51 | 1.17 |
| 1,048,576 | Parse decimal | 48.14 | 48.10 | 4.66 |
| 1,048,576 | Render decimal | 110.72 | 109.48 | 14.32 |

The selected large short-quotient cases improve by 1.9–4.5× and still
take about 8.4–9.0× Malachite's time. The crossover depends on transform
padding. Increasing the quarter-width case from 262,144 to 262,400
bits raises the estimated quotient from 1,024 to 1,025 limbs. Its
correction then falls below the occupancy cutoff. At 524,288 bits the
third-width case also falls back because the correction misses the
direct occupancy cutoff and its operand ratio is at most 2:1. Some
controls moved during the broader sweep while the host was running
validation. The 262,400-bit quarter-width case measured 7.84 to 8.50
ms there. The repeat reproduced the crossover without that timing
increase (`repeat.csv`).

## Diagnostics

Three matched four- and eight-operation counter pairs for the million-bit
quarter-width case reduced median user-mode cycles from 442.23 million to
108.73 million per operation. Instructions fell from 2.254 billion to
0.496 billion. In sampled profiles the share in the Algorithm D digit
loop fell from 97.9% to 26.2%. The candidate spends much of the remaining
time in the transform's modular arithmetic and collection storage.

The Algorithm D row still uses both `imul` and `mul`, materializes the
borrow bits, and checks the divisor index inside the loop. The matching
GCC 16.2.1 C row uses one `mul`, consumes the condition flags with
`adc`, and checks the length at entry. The existing
[Plenty](../latency/kernel.plenty) and [C](../wide-division/kernel.c)
probes produced identical checksums. Assembly and the counter pairs are
retained with the measurements. Three counter pairs put the Plenty row
at 32.03 instructions and 7.37 cycles per limb, and the C row at 14.02
instructions and 4.61 cycles per limb on this host.

## Reproduction

Compile the same [benchmark](../bench.plenty) with separate release
compilers from the baseline and candidate. Build the locked Malachite
comparator from [the manifest](../Cargo.toml). Use the shape sweep to
select quotient widths and product ratios. The runner preserves the
original workloads when the flag is omitted.

```sh
python3 -B experiments/bigint-comparison/block-division/measure.py results.csv \
  main=BASELINE candidate=CANDIDATE malachite=MALACHITE --cpu 0 \
  --bits 8192 65536 131072 262144 262400 327680 393216 524288 1048576 \
  --shape-sweep \
  --cases div/balanced div/short div/quotient-1-2 div/quotient-1-3 \
          div/quotient-1-8 div/quotient-1-16 mul/dense mul/ratio-2 mul/ratio-3 \
          mul/ratio-4 mul/ratio-8 mul/ratio-16 parse/decimal render/decimal \
          render/ternary render/base36
```

The primary checkout's `target/issue-987-short-division/` retains raw
CSV files, the exact executables, profiles, and validation logs.

## Correctness

The existing 195-case Malachite corpus matched all 16,378 lines. Python
independently checked 160 exact quotient/remainder pairs and 121 exact
signed products across the block, transform-size and occupancy
boundaries. The cases include zero low divisor limbs, remainders at
both endpoints, both operand orders and all sign combinations. The
regression suite covers a short estimate that is two too high, a
saturated quotient, normalization and signed floor correction, and a
carry-heavy skewed product with an odd final block.

The full Plenty suite passed 7,904 tests with nine ignored. Strict
all-target, all-feature Clippy and formatting checks passed. The full
documentation checks passed, including 396 Python tests with four skipped
and 3,556 published pages built with the pinned mdBook 0.5.4.
