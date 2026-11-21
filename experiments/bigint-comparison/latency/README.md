# Bigint division and rendering latency (issue #987)

PR #1404 brought balanced division from 69.6M to 31.1M instructions at
65,536 bits, but time rose from about 3.46 to 4.99 ms. The investigation
here uses wall-clock time to select the implementation. Both Plenty
builds use the same optimized compiler sources. The baseline library is
commit `492ca87a`.

## Algorithm trace

The pinned [Malachite 0.14.0 crate](https://docs.rs/crate/malachite-nz/0.14.0)
dispatches among schoolbook, divide-and-conquer and Barrett division in
`src/natural/arithmetic/div_mod.rs`. Its schoolbook path estimates a
quotient limb using the top two divisor limbs and a precomputed inverse.
For large blocks it divides by the high part of the divisor and multiplies
the quotient estimate by the omitted low part to correct the remainder.
The larger Barrett path computes quotient blocks by multiplying their
high partial remainders by an approximate inverse. Its correction needs
only the low divisor-length-plus-one limbs of a second product. The
64-bit dispatch constants include 61 and 809 limbs, and the choice
also depends on operand shape. The [earlier trace](../radix-render/README.md)
records radix-specific thresholds.

At rendering leaves, Malachite extracts a word-sized power such as 10¹⁹
using normalized reciprocal division. It then unpacks the short remainder
using fixed-point multiplication. The Plenty change takes the first step.
It reuses the existing two-by-one quotient estimator, computing one
normalized chunk and reciprocal per render. For a normalization shift
`s`, the divisor and the shifted dividend are divisible by 2ˢ, so the
partial remainder stays divisible by 2ˢ throughout the loop. The quotient
limbs need no unshifting. The chunk remainder is unshifted once at the
end of the scan. A chunk divisor occupies one limb, so each pass can
remove at most one leading limb. Decimal extraction consumes 19 digits
per scan.

## Trial-product row

Split `q * v[i]` into its high and low limbs. Subtract the low limb and
the incoming borrow from the old row limb. Each subtraction's borrow bit
is a function of its operands' top bits and the wrapped difference's top
bit. Add the two bits to the high limb to get the next incoming borrow.
The resulting borrow fits one limb because the full product plus the
incoming borrow is below the square of the limb base.

The existing [row kernel](../wide-division/kernel.plenty) and matching
[C kernel](../wide-division/kernel.c) provide the controls. The [branch-free
kernel](kernel.plenty) uses the same 1,024-limb inputs and gives
the following hardware counter medians across five pinned batches:

| Kernel | Instructions per limb | Cycles per limb |
| --- | ---: | ---: |
| Original Plenty row | 25.85 | 12.89 |
| Branch-free borrow | 32.14 | 9.89 |
| GCC 16.2.1 row | 14.05 | 5.57 |

The old loop uses two data-dependent compare-and-branch paths to
materialize carry bits. The new loop uses a pair of bitwise calculations
and shifts. Both still emit separate low and high multiply instructions
and keep the divisor length check. GCC uses flags directly through
`adc`. The new code keeps the same checked views in Plenty.
The sweep tried the prior row at or below 32, 64, 128 and 256 limbs.
The cutoff variants traded gains at one size for losses at another,
so the implementation uses one row for all multiword divisors.

## Recursive prototype

An in-place prototype tried Malachite's divide-and-conquer quotient
blocks with the existing Plenty multiplication. Each block divides the
upper part of the remainder by the top part of the divisor, then
multiplies by the omitted low limbs and corrects by adding the divisor
back as needed. The prototype shared the normalized numerator and
quotient slices and allocated the low product through existing
multiplication. It used schoolbook division when the truncated estimate
could overflow its quotient block.

Thresholds of 16, 32, 64, 128 and 256 quotient limbs were timed with the
same branch-free row. Each split also required a divisor at least
twice the threshold. Even the fastest recursive candidate took 2.95 ms
for 65,536-bit division and 8.32 ms for 131,072-bit division. The
nonrecursive candidate took 1.68 ms and 6.46 ms respectively in that
sweep. The earlier recursive and full-width reciprocal prototypes
also lost on elapsed time. A low-product kernel and reusable
multiplication scratch are useful pieces to measure before adopting
subquadratic division. The delivered division remains quadratic.

The 65,536-bit decimal render profile sampled about 21% of cycles in
the long-division digit loop and 10% in the two-by-one estimate. The
report resolves symbols without attributing inlined work to separate
functions.

## Elapsed time

Median milliseconds per operation on the Ryzen 7 7840HS reference host.
Optimized executables were pinned to CPU 0 and interleaved across seven
timed batches after warmup. The timed loop excludes input parsing.
Division uses a product of two same-width factors as the numerator and
one factor as the divisor. Batch checksums matched across engines.

| Operand bits | Operation | Before PR #1404 | `main` | Candidate | Malachite |
| ---: | --- | ---: | ---: | ---: | ---: |
| 2,048 | Division | 0.0145 | 0.0074 | 0.0078 | 0.0009 |
| 2,048 | Decimal render | 0.0278 | 0.0272 | 0.0196 | 0.0023 |
| 8,192 | Division | 0.0888 | 0.0400 | 0.0438 | 0.0107 |
| 8,192 | Decimal render | 0.1974 | 0.1742 | 0.1164 | 0.0127 |
| 16,384 | Division | 0.2712 | 0.2986 | 0.1312 | 0.0340 |
| 16,384 | Decimal render | 0.4875 | 0.3951 | 0.2786 | 0.0376 |
| 32,768 | Division | 0.9513 | 1.2899 | 0.4630 | 0.1058 |
| 32,768 | Decimal render | 1.3497 | 1.2537 | 0.8705 | 0.1097 |
| 65,536 | Division | 3.4947 | 5.0483 | 1.6544 | 0.3041 |
| 65,536 | Decimal render | 4.0546 | 3.7822 | 2.3130 | 0.3388 |
| 131,072 | Division | 13.3069 | 19.9154 | 6.4748 | 0.6746 |
| 131,072 | Decimal render | 11.6318 | 11.1789 | 6.0458 | 0.9197 |

At 65,536 bits, balanced division takes 67% less time than `main`
and 53% less time than before PR #1404. Decimal rendering takes 39%
less time than `main`. At the two intermediate division sizes in the
table, 2,048 and 8,192 bits, elapsed time rises by roughly 5% and 9%.
The measured 16- and 256-bit cases were within 0.3 μs of `main`.

## Correctness

The Malachite differential suite matched all 16,378 outputs across
195 inputs. Python's integer divmod matched 3,904 signed cases with
divisors up to 131,072 bits. The radix cap regression includes 19-,
20- and 38-digit decimal boundaries and runs in both compiler modes.

## Reproduction

Use the [bigint harness](../wide-division/README.md#reproduction) with
release compilers from the baseline and this branch. The benchmark reports
elapsed time for its operation loop after parsing operands. Use optimized
executables pinned with `taskset`, interleave engines, discard one warmup
batch per engine, and take the median of the elapsed nanoseconds divided
by the batch operation count. Confirm matching checksums for every batch.
For the row comparison, call the kernel binaries with
`16384 14501418449037603789` under `perf stat -e cycles:u,instructions:u`
and divide the counters by 16,777,216.
