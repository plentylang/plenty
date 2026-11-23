# Bigint radix parsing and radix-power squaring

## Algorithm trace

Malachite 0.14.0 uses direct limb packing for power-of-two radices.
Its decimal parser uses an inline limb for short input and a grouped
conversion for larger input. The non-power-of-two base case builds one
limb buffer with Horner steps in powers of the input radix that fit in
one limb. The 64-bit decimal group is 19 digits. A divide-and-conquer
parser starts at 7,100 digits, reuses the power table and writes each
combined product into its output buffer. Power-table rows omit whole
zero low limbs and track their shift. The table builder uses dedicated
squaring. The transform squarer reuses the forward transform of the
single operand. See the pinned source in
[the string parser](https://docs.rs/crate/malachite-nz/0.14.0/source/src/natural/conversion/string/from_string.rs),
[the digit converter](https://docs.rs/crate/malachite-nz/0.14.0/source/src/natural/conversion/digits/general_digits.rs),
and [the transform multiplier](https://docs.rs/crate/malachite-nz/0.14.0/source/src/natural/arithmetic/mul/fft/mod.rs).

Plenty previously split down to 12 digits and constructed BigInt
temporaries at every internal node. It now validates the original
ASCII input, including the digit cap, before arithmetic. It strips
leading zeros before allocating. Bases 2, 4, 8, 16, and 32 pack bits
directly into limbs in linear time. In other bases a grouped leaf uses
one word buffer. Each group changes the accumulated value from `x`
to `x * factor + digit_group`, where `factor` is the greatest power
of the radix below the limb base. Fusing the multiplication and
addition passes the group in as the first carry. A sweep of 4, 8, 16,
32, 64, and 112 groups per leaf favored 112 overall on this host,
including the three largest ternary cases.

Large inputs keep the same right-hand digit width at each depth and
share its radix power. The left width may grow by one relative to
twice the next split at each level because of rounding. The leaf
cutoff is larger than this rounding accumulation across an i64-sized
input. Combining uses a limb product and adds the low child into that
same buffer. If the high child has `a` limbs and the split power
`P` has `p` limbs, then the result fits `a + p` limbs because the
low child is below `P`. Whole low zero limbs in `P` can be omitted
unless that would make a balanced multiplication fall back to
schoolbook. Radix-power squaring also omits whole low zero limbs.
On transform sizes it squares the residues from one forward transform
per prime, then uses the existing inverse transform and CRT.

## Elapsed time

The table gives milliseconds per operation, using the median of seven
interleaved batches and the same operands across all three executables.
The baseline was built in a separate checkout of `a24f9530`. All timed
batches agreed on the result checksum.

| Bits | Operation | Plenty baseline | Plenty with this change | Malachite 0.14.0 |
| ---: | --- | ---: | ---: | ---: |
| 65,536 | Parse decimal | 3.600 | 1.172 | 0.131 |
| 65,536 | Parse hex | 3.510 | 0.062 | 0.018 |
| 1,048,576 | Parse decimal | 114.68 | 47.27 | 4.53 |
| 1,048,576 | Parse ternary | 122.72 | 67.70 | 7.47 |
| 1,048,576 | Parse base 12 | 115.95 | 66.04 | 5.27 |
| 1,048,576 | Parse base 36 | 115.55 | 50.50 | 5.93 |
| 1,048,576 | Parse octal | 110.55 | 0.92 | 0.32 |
| 1,048,576 | Parse hex | 114.25 | 1.40 | 0.82 |
| 1,048,576 | Render decimal | 114.45 | 104.76 | 13.80 |
| 1,048,576 | Render ternary | 174.52 | 173.25 | 16.20 |
| 1,048,576 | Render base 36 | 120.85 | 116.19 | 11.99 |
| 1,048,576 | Dense multiply | 13.92 | 13.80 | 1.99 |
| 1,048,576 | Balanced divide | 68.57 | 68.11 | 6.64 |
| 1,048,576 | Short-quotient divide | 93.27 | 93.06 | 2.45 |

The million-bit inputs parse 2.43× faster in decimal and 81× faster in
hex. Decimal parsing still takes about 10× as long as Malachite. The
power-of-two gap is smaller, at 1.7× for hex and 2.8× for octal.

The broad run started during suite validation. A quieter repeat reproduced
the large-input gains. At 8,192 bits, decimal parsing took 0.036 ms
versus 0.277 ms. At 1,048,576 bits, it took 47.59 ms versus 114.85 ms,
and decimal rendering took 105.07 ms versus 113.93 ms. The balanced
division and dense multiplication controls varied in either direction.
At 65,536 bits, dense multiplication took 0.743 ms versus 0.705 ms in
the repeat and about 0.720 ms in both builds in the broad run. A separate
small-input repeat found no material change at 16 bits and faster parsing
across the six measured radices at 64 bits.

## Diagnostics

In the isolated grouped leaf kernel, the Plenty loop has 20
instructions per limb and the equivalent GCC 16.2.1 `-O2` loop
using a wide multiply-add has nine. The Plenty loop has no per-limb
bounds check. It emits both `imul` and `mul`, computes the carry
with separate word operations where GCC uses `add` and `adc`,
checks the high-word addition, and moves the carry around the loop
back edge. The full-row checksums agree. Three CPU-pinned runs
averaged 4.48 billion cycles for Plenty and 1.54 billion for C on
565 million limb updates. This probe identifies work in a base-case
kernel. Operation-level elapsed times decide the dispatch.

A sampled cycle profile of 524,288-bit decimal parsing attributed 16.2%
to schoolbook multiplication and about 42% to list reservation, append,
address lookup, storage and growth helpers. The grouped multiply-add
row accounted for 1.4%. The larger gap remains in the recursive
combination and its scratch storage.

The exploratory grouped-only and grouped-plus-square builds used the
same candidate source and the same release compiler, with the radix
power changes toggled together. At 1,048,576 bits the latter improved
median decimal rendering from 112.82 ms to 103.44 ms and decimal
parsing from 48.64 ms to 46.73 ms. Base-36 parsing took 49.62 ms
versus 48.20 ms in this ablation. These results cover the combination
of omitted zero limbs, in-place multiplication by the extra radix
factor for odd exponents, and the one-forward-transform square.

## Reproduction

Use the same [benchmark](../bench.plenty) with release compilers from
the pre-change commit `a24f9530` and from this branch. Build the locked
Malachite comparator from [the standalone manifest](../Cargo.toml).
Pass the optimized executables to the existing pinned, interleaved
runner. Its order, operand generation and warmup are shared across
executables. The parse loop times the conversion and bit-length
checksum. Operand loading and the preparation of the text are outside
the timer.

```sh
python3 -B experiments/bigint-comparison/block-division/measure.py final.csv \
  main=BASELINE candidate=CANDIDATE malachite=MALACHITE --cpu 0 \
  --bits 16 32 64 256 2048 8192 32768 65536 131072 262144 524288 1048576

python3 -B experiments/bigint-comparison/block-division/measure.py quiet.csv \
  main=BASELINE candidate=CANDIDATE malachite=MALACHITE --cpu 0 \
  --bits 256 2048 8192 32768 65536 1048576 \
  --cases parse/decimal parse/ternary parse/base12 parse/base36 parse/hex \
          parse/octal render/decimal mul/dense div/balanced
```

The checkout's `target/issue-987-parse-evidence/` retains the raw
observations, executables, source for the isolated limb probe, and
qualification logs. The measurements apply to the Linux x86-64
Ryzen 7 7840HS host.

## Correctness

The Malachite corpus matched all 16,378 output lines across 195
cases. Python independently checked 791 exact parsed integers, including
all supported radices, word and leaf boundaries, signed inputs,
leading zeros, and random and sparse inputs through 280,019 digits.
Both compiler modes have regressions for case-insensitive digits,
crossings of 64-bit limbs, the cap with leading zeros, and a
120,001-digit signed base-36 parse compared with a separately
computed power. Allocation injection covers failure in grouped
and bit-packed parsing and allocation-free decimal `i64.MIN`
and signed zero.

`just test` passed, including all 7,169 integration tests. Workspace
Clippy passed with warnings denied. The docs source-link check,
396-test Python check, and pinned mdBook 0.5.4 site build passed.
