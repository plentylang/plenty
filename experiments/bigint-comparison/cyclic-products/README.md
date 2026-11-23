# Close-fitting cyclic bigint products

## Recovering the exact correction

Malachite 0.14.0 uses multiplication modulo `B^C-1` both when building
the Newton reciprocal and when subtracting a Barrett quotient block.
Its modulus-size helper rounds up in limb increments of at most eight.
It can split an even-length product using the factors `B^(C/2)-1` and
`B^(C/2)+1`, then reconstruct the residue. See `mul/mul_mod.rs`,
`limbs_invert_approx`, and `limbs_div_barrett_large_product` in the
[pinned Malachite crate](https://docs.rs/crate/malachite-nz/0.14.0).

Plenty already has an exact cyclic convolution on power-of-two limb
widths. A prototype using Malachite's split with Plenty's existing linear
transform at the leaves had mixed timings. The delivered method keeps
the power-of-two convolution and adds an exact short low product when
the width is just below the divisor or Newton residual.

Let `B = 2^64`. The exact difference `R` is nonnegative and bounded
by `5*B^n` for Barrett correction or `6*B^n` for a Newton residual.
The earlier Barrett path required a modulus above its bound. Newton
always formed the full product. For the smaller cyclic width `C <= n`,
set `g = n+1-C`. With the cyclic
residue `A` represented in `[0, B^C-1)`, write

```text
R = A + t*(B^C-1),  0 <= t < B^g.
```

The bound on `R` gives the bound on `t`, since `B^g*(B^C-1)`
exceeds both residual bounds. With `g <= C`, `B^C-1` is congruent to
minus one modulo `B^g`. That gives `t = (A-R) mod B^g`. The low `g`
limbs of the original difference suffice. Reconstruction writes `t`
at limb `C` and subtracts it from the low row, propagating any borrow
into that high row. If the cyclic convolution returns all-one limbs
for zero, canonicalize them before this calculation.

The short product uses schoolbook multiplication for small windows.
For larger windows it computes the full low-half product and the
needed prefixes of the two cross-products. An odd output width also
keeps one limb of the high-by-high product. The dispatch bounds the
window to one eighth of the cyclic width plus one limb. At a divisor
width of 16,384 limbs the short product is one limb. A Newton step
can use the same reconstruction because its minuend is a single
power of `B`, zero in the short low window. The exponent wraps
modulo `C` for the cyclic residue.

## Experiments

The guarded high-half product omits the low-by-low term and keeps
the high and cross products down through the guard limb. These terms
discard less than four units of the guard limb's place value, so the
high half stays exact or one below. The Barrett correction loop can
absorb that extra unit. With Plenty's packed transforms, the three
smaller products usually cost more than the full product. The full
quotient-estimate multiplication remains in use. The short-product
analysis comes from [Harvey and Zimmermann](https://members.loria.fr/PZimmermann/papers/div-final.pdf).

The scalar radix-4 trials fused the two full-transform stages while
preserving the radix-2 ordering used by the truncated transform.
The first variant kept canonical field residues after each operation.
A second allowed residues below twice the prime between stages and
specialized the size-four butterflies. A third tested the size-four
butterflies alone. They remain experiments because the measurements
show gains on some sizes and losses on others.

## Measurements

AMD Ryzen 7 7840HS, Linux x86-64, Rust 1.100.0, optimized builds.
Times are median elapsed milliseconds per operation across seven
interleaved batches pinned to CPU 10. Malachite 0.14.0 was built for
`x86-64-v3`. Division times include reciprocal construction.

| Divisor or value bits | Workload | Prior Plenty | Candidate | Malachite |
| ---: | --- | ---: | ---: | ---: |
| 524,224 | Balanced floor divmod | 39.19 | 37.41 | 4.33 |
| 524,288 | Balanced floor divmod | 36.34 | 28.54 | 3.38 |
| 524,352 | Balanced floor divmod | 40.11 | 39.06 | 3.83 |
| 589,824 | Balanced floor divmod | 43.01 | 35.24 | 4.00 |
| 589,888 | Balanced floor divmod | 43.42 | 43.27 | 4.02 |
| 1,048,512 | Balanced floor divmod | 60.09 | 54.48 | 8.73 |
| 1,048,576 | Balanced floor divmod | 82.32 | 53.39 | 8.39 |
| 1,048,640 | Balanced floor divmod | 127.00 | 77.50 | 10.52 |
| 1,179,648 | Balanced floor divmod | 99.55 | 69.38 | 8.17 |
| 1,179,712 | Balanced floor divmod | 97.68 | 100.80 | 8.03 |
| 1,048,576 | Floor divmod, double-width quotient | 224.35 | 130.08 | 17.72 |
| 1,048,576 | Decimal rendering | 180.58 | 176.35 | 20.30 |
| 1,572,864 | Decimal rendering | 225.59 | 184.52 | 23.97 |

In the repeat sweep, balanced 1,048,576-bit division took 85.62 and
55.64 ms, and the 1,048,640-bit case took 85.28 and 56.02 ms.
Decimal rendering at 1,572,864 bits took 223.64 and 188.06 ms.
Other rendering sizes were generally close to baseline. Dense
multiplication, decimal parsing, and short-quotient controls at
65,536 through 1,048,576 bits stayed within 4% of baseline. The
65,536-bit double-width-quotient control was 4.6% slower in the
first sweep and within 0.3% in an eleven-batch repeat.

A separate 12-operation million-bit division control counted 19.30
and 12.28 billion userspace instructions per process. Cycle counts
were 4.33–4.44 and 2.91–3.03 billion, respectively. The previous
base-case division loop accounted for about 35% of baseline cycle
samples and less than 1% in the candidate. Peak resident memory in
one process per version was 4,104 and 4,508 KiB.

The radix-4 prototype with canonical residues took 28.80 ms for
million-bit dense multiplication versus 24.22 ms in the same
sweep. The lazy-residue variant improved that workload from
15.71 to 14.81 ms in a separate sweep, while the 1,572,864-bit
case slowed from 35.62 to 38.55 ms. For the guarded high-product
trial, million-bit balanced division took 64.77 ms with the
guarded product and 56.40 ms with the full product in the same
build family.

A Malachite ablation used two generic `x86-64` builds. One
kept runtime AVX2/FMA dispatch. For the other, a temporary copy
suppressed the `fft_avx2` configuration in the crate's build
script. At 1,048,576 bits dense multiplication took 2.17 ms
with dispatch and 43.45 ms without. The baseline without
dispatch also loses hardware fused operations, so this measures
the combined path. Plenty's scalar trials used the existing integer
Montgomery transform throughout. A small-prime hardware FFT in
Plenty remains a separate experiment.

All benchmarks check result checksums. Separate exact checks against
Python cover 360 signed divisions and 512 direct wraparound cases,
including zero residues and borrow across the split. Another oracle
checks 240 reciprocal bounds, 240 products and 308 divisions.
The regression runs the public division path in both optimized
and fast-compile modes.

## Reproduction

Build optimized compilers at baseline `d247327e` and at the candidate
revision, and compile `experiments/bigint-comparison/bench.plenty` with
each compiler using `--compile --no-cache --no-strip`. Build the
Malachite comparator from the lockfile in `experiments/bigint-comparison`.
Then run the shared benchmark driver:

```sh
python3 -B experiments/bigint-comparison/block-division/measure.py \
  times.csv baseline=baseline-bench candidate=candidate-bench \
  malachite=plenty-bigint-comparison --cpu 10 --batches 7 --shape-sweep \
  --bits 393216 524224 524288 524352 589824 589888 786432 1048512 \
         1048576 1048640 1114112 1179648 1179712 1572864 \
  --cases div/balanced render/decimal
```

The driver excludes operand reading and parsing from the measured
division and rendering loops. It interleaves pinned executables, checks
their checksums, drops one warmup batch, and records every measured
elapsed time. The test suite also checks exact quotient and remainder
values with independently constructed numerators.
