# Bigint 64-bit division (issue #987)

Baseline `9b5ce34e` is the merge of the [grouped radix renderer](../radix-render/README.md).
Both Plenty builds use the same release compiler sources, with the candidate
embedding the changed library. Malachite 0.14.0 is the existing development
comparator. Its basecase uses normalized leading-limb inversion, and its
larger balanced division and radix splits use multi-limb Barrett inverses.
The [prior trace](../radix-render/README.md#malachites-conversion-path) records
the source paths and dispatch thresholds.

## Algorithm

Algorithm D runs directly on 64-bit limbs. Both operands are shifted so
the top divisor limb is at least 2⁶³. Let `B = 2^64` and `d` be that limb.
The reciprocal is `floor((B² − 1)/d) − B`. The highest limb of `B² − 1`
is `B − 1`. Its division by `d` yields one with remainder `B − 1 − d`.
The remaining 64 quotient bits follow by shifting in the low limb's ones
and conditionally subtracting `d`.

For a two-limb numerator `N` with its high limb below `d`, multiply by the
full reciprocal `floor((B² − 1)/d)` and keep the part above `B²`. The
reciprocal is at most one below `B²/d`, so the estimate differs from the exact
quotient by at most one. Subtract the trial product to recover the remainder
and increment the estimate when the remainder is at least `d`. When the
numerator's high limb equals `d`, start with `B − 1`. Algorithm D then uses
the second divisor limb to reduce the trial quotient and adds the divisor
back if the trial product borrowed from the high numerator limb.

If the divisor ends in `s` zero limbs, write it as `b_high * B^s` and the
dividend as `a_high * B^s + a_low`. Divide `a_high` by `b_high` and restore
`a_low` below the high remainder after undoing normalization. This reduces
the work for even radix powers. Each conversion prepares a divisor once per
split exponent and reuses its normalized digits and leading-limb reciprocal.
Wide division starts at two stored divisor limbs. The cutoff sweep covered
divisors from 2 through 64 limbs with both balanced and one-limb quotients.
The prototype used 25,476 instructions for the two-limb divisor and one-limb
quotient, down from 29,036.

## Measurements

User-mode instructions per API operation, in millions. These are medians of
three paired runs pinned to one Linux x86-64 CPU. Each pair subtracts the
counts for two and four operations to remove fixed startup and setup work.
Division divides the product of two same-width inputs by one factor.
Rendering converts one input to decimal. Checksums agreed across engines.

| Input bits | Operation | Baseline | Candidate | Malachite |
| ---: | --- | ---: | ---: | ---: |
| 16,384 | Division | 5.62 | 2.62 | 0.51 |
| 16,384 | Render | 8.04 | 6.11 | 0.52 |
| 32,768 | Division | 19.08 | 8.66 | 1.56 |
| 32,768 | Render | 22.32 | 14.72 | 1.54 |
| 65,536 | Division | 69.61 | 31.05 | 4.54 |
| 65,536 | Render | 67.06 | 39.07 | 4.44 |
| 131,072 | Division | 264.99 | 117.14 | 9.22 |
| 131,072 | Render | 210.08 | 106.44 | 12.45 |

The existing 62-workload harness agreed with the large probe at 65,536 bits.
It found 25.5%, 45.9% and 51.6% fewer division instructions at 256, 2,048
and 8,192 bits respectively. At 8,192 bits decimal render used 12.9% fewer.
Addition, multiplication and parsing counts stayed within 0.2% of baseline.
At 65,536 bits the candidate still uses about 6.8× Malachite's instructions
for division and 8.8× for decimal rendering. With the compile cache off,
building the full Plenty benchmark used 922.3M instructions, up from 866.3M
(6.5% more).

Two subquadratic prototypes used Plenty's current multiplication. At 65,536
bits, Newton-built full-width reciprocals with full products took 119.3M
instructions for division and 123.8M for rendering. Burnikel–Ziegler blocked
division took 101.5M and 98.5M. At 131,072 bits both prototypes still cost
more than the baseline for both operations. These measurements leave
multi-limb reciprocals and faster division open. Malachite's use of
truncated products and reused scratch adds avenues absent from these
prototypes.

## Correctness

The Malachite differential suite matched all 16,378 outputs across 195 inputs.
A Python integer oracle checked 3,904 signed divisions, including divisors up to
131,072 bits, exact divisions, zero low limbs and remainders at the endpoints.
A separate check compared 100,000 leading-limb reciprocal estimates against
exact two-by-one division. The repository tests exercise Algorithm D's
preliminary decrement, saturated estimate and final add-back, along with
remainder reconstruction and floor semantics in both compiler modes.

## Trial-product row

The [Plenty kernel](kernel.plenty) is extracted from the production row.
The [C kernel](kernel.c) uses GCC 16.2.1, a runtime trial quotient, a writable
pointer and a divisor length guard. Matching checksums and three pairs of
128/256 rows measured 26.03 instructions per digit in Plenty and 14.02 in C.
Relative to C's loop, Plenty adds three instructions for separate loading
and widening multiplication, two for the divisor bounds check, one for row
arithmetic, and five to seven for materializing and adding the two carry bits.
C uses `adc` to consume the carry flags directly. Processing 64-bit limbs
roughly quarters the number of row iterations for balanced division.

## Reproduction

Follow the [existing harness commands](../radix-render/README.md#reproduce)
using release compilers from the baseline and this branch. Use the 65,536-bit
inputs from the harness. The larger probe uses Python's `random.Random(987)`
and draws an `n−1` bit value twice for each width in
`[16384, 24576, 32768, 40960, 49152, 81920, 98304, 114688, 131072]`, setting
the top bit each time. Use `bench-candidate div a_times_b b count` and
`bench-candidate render a b count` with hexadecimal operands. Subtract
`perf stat -e instructions:u` totals for counts two and four and divide by two.

Compile the extracted kernels with `plenty --no-strip --compile kernel.plenty`
and `gcc -O3 -march=native kernel.c`. Run both with the same row count and
trial quotient. The optimized Plenty assembly is available from
`plenty --dump asm --dump-filter subtract_wide_product kernel.plenty`.
