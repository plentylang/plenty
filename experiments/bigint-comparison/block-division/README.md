# Recursive bigint division and radix reciprocals

## Algorithm trace

[Malachite 0.14.0](https://docs.rs/crate/malachite-nz/0.14.0) switches
between schoolbook, divide-and-conquer and Barrett division. The pinned
source is in `natural/arithmetic/div_mod.rs`. Its 64-bit thresholds
include 61 limbs for divide-and-conquer and 809 for ordinary Barrett
division, with additional dispatch based on quotient length and balance.
The rendering path in `natural/conversion/digits/general_digits.rs`
prepares one reciprocal per qualifying radix power and reuses it across
splits. Malachite uses an approximate Newton inverse and, for large
corrections, a product modulo a power of the limb base minus one.

Plenty keeps Algorithm D for short quotients and smaller operands. A
large balanced division sets aside one quotient digit to establish the
normalized remainder invariant. It then processes blocks of up to
`ceil(n / 2)` quotient limbs for an `n`-limb divisor. The tentative
quotient comes from dividing by the leading divisor limbs. The omitted
low divisor limbs contribute one correction product. Because the
truncated divisor is normalized, the estimate is at most two too high.
When the truncated prefix would overflow the block, the trial block is
all ones and its truncated remainder fits in an extra limb. The solver
shares one correction-product buffer through recursion. For balanced
lengths the recurrence is two half-size divisions plus two half-size
multiplications. The existing Karatsuba, Toom-3 and NTT paths make this
subquadratic.

Rendering shares a right-hand split width at each depth. Its exact
powers come from squaring the power for the halved exponent, with one
extra radix factor for odd exponents. A radix power with repeated,
qualifying splits can also cache a Barrett reciprocal. Construction
divides an all-ones numerator by an upward-rounded divisor prefix,
then discards one guard limb. This uses the recursive solver. The cache
also holds the cyclic transform width, a power of two greater than the
number of significant divisor limbs.

Let `B = 2^64`, `D` be a normalized `n`-limb divisor, and `k` the
next quotient block width. Write the current dividend as `X = R*B^k + L`
with `R < D`, and let `H` be the high `k` limbs of `R`. The rounded
reciprocal `I` is at most `B^(n+k)/D`, with error below two. Its leading
limb is implicit. The trial quotient is `floor(H*I/B^k)`. The omitted
part of `X` is less than `B^n`, and `D >= B^n/2`, so the trial
remainder lies in `[0, 5D)`. The product `D*trial` and `X` can both
be reduced modulo `B^m - 1`, where `m > n` is the cyclic width.
Their difference determines the full trial remainder, since `5D` is
smaller than the modulus. Subtracting `D` as needed finishes the block.

The cyclic convolution reuses the exact packed transform and folds its
final carry back into the first words. It handles only blocks for which a
linear product would need a larger transform width. Skewed short tails
continue with Algorithm D. The cache is conversion-local and reserved for
repeated splits with at least 2,049 significant divisor limbs and a
quotient more than one quarter wider, a shape found in long even-radix
rendering after omitting zero divisor limbs. Preparation starts below
the root, where several splits can share a radix power.

## Measurements

Median elapsed milliseconds per operation on a Ryzen 7 7840HS under Linux
x86-64, pinned to CPU 0. The division sizes describe both divisor and
quotient for balanced divmod and the divisor for a short quotient. The
short quotient has one quarter as many bits as the divisor in this run.

| Workload | Bits | Prior Plenty | This change | Malachite 0.14.0 |
| --- | ---: | ---: | ---: | ---: |
| Balanced divmod | 98,304 | 3.45 | 3.64 | 0.58 |
| Balanced divmod | 131,072 | 6.60 | 5.34 | 0.67 |
| Balanced divmod | 262,144 | 33.12 | 18.04 | 1.98 |
| Balanced divmod | 524,288 | 124.17 | 41.08 | 4.39 |
| Balanced divmod | 1,048,576 | 557.44 | 104.11 | 14.02 |
| Short-quotient divmod | 524,288 | 31.29 | 34.81 | 1.50 |
| Short-quotient divmod | 1,048,576 | 134.27 | 131.51 | 3.73 |
| Decimal render | 524,288 | 86.17 | 70.98 | 7.30 |
| Decimal render | 1,048,576 | 309.17 | 183.41 | 20.49 |
| Ternary render | 1,048,576 | 342.76 | 187.08 | 17.26 |
| Base-36 render | 1,048,576 | 187.74 | 126.30 | 12.23 |
| Decimal parse | 1,048,576 | 229.59 | 116.30 | 4.68 |
| Dense multiply | 393,216 | 30.81 | 8.12 | 0.83 |

The first measured recursive size, 98,304 bits, took 5.5% longer than
Algorithm D. At 131,072 bits the recursion saved 19%, and the advantage
grew to 5.35× at 1,048,576 bits. The 8,192-bit base-36 render took 8%
longer in this run. Decimal parsing and dense multiplication also
benefited at larger sizes.

A second run used the same harness with a smaller matrix and different
dense operands. Balanced 1,048,576-bit divmod measured 381.80 ms for
prior Plenty, 70.97 ms for the candidate, and 7.13 ms for Malachite.
Decimal rendering measured 217.54 ms, 117.96 ms, and 14.10 ms in the
same order. Short quotients use Algorithm D. The first run saw about
11% longer elapsed time at 262,144, 524,288 and 786,432 bits. The
second run put short quotients at 262,144, 524,288 and 1,048,576 bits
within 2% of prior Plenty. The emitted instructions in the Algorithm D
loop match apart from relocation addresses.

The reciprocal ablation ran separately with two locally built copies of
the current module. It disabled reciprocal preparation in one copy by
setting `RADIX_RECIPROCAL_LIMBS` to `MAX_TRANSFORM_WORDS`. Those builds
were within 1.4% through 786,432 bits. At 1,048,576 bits, median decimal
rendering fell from 121.77 ms to 116.13 ms (4.6%). Base-36 rendering
fell from 132.29 ms to 122.83 ms (7.2%). The comparison keeps the new
quotient blocks and shared radix powers in both builds.

The packed-transform kernel probe was diagnostic. Its word-arithmetic
version used 11% more retired instructions and 11% fewer CPU cycles
than the prior Plenty kernel. Retired branches fell by 31% in the same
probe. Dispatch decisions used the whole-operation elapsed times.

## Correctness

The integration tests run a saturated recursive quotient block, signed
floor division, and a 316,000-digit conversion at its exact digit cap
in both compiler modes. An independent Python oracle checked 720 signed
quotient/remainder pairs through 8,192 significant divisor limbs and 45
radix cases through 556,623 digits in bases 3, 10 and 36. Another 45
direct checks forced the cached reciprocal path, including an all-ones
prefix, several quotient widths, and remainders of zero and `D - 1`.
The existing Malachite oracle matched 16,378 output lines across 195
cases. The full suite passed with 7,167 integration tests, 345 compiler
tests, 376 runtime tests, nine compile-benchmark tests, and two binary
tests. All-target, all-feature workspace Clippy and the three docs CI
checks passed, including the pinned mdBook site check.

## Reproduction

Build the baseline (`58ce2052`) and candidate Plenty compilers in separate
worktrees. Compile the same current `bench.plenty` with each release
compiler. Build the Malachite comparator from the locked manifest in
`experiments/bigint-comparison/`. From the repository root, run the
comparison with the three executable paths.

```sh
python3 -B experiments/bigint-comparison/block-division/measure.py results.csv \
  main=BASELINE candidate=CANDIDATE malachite=MALACHITE --cpu 0 \
  --bits 2048 8192 32768 65536 98304 131072 196608 262144 393216 524288 786432 1048576

python3 -B experiments/bigint-comparison/block-division/measure.py repeat.csv \
  main=BASELINE candidate=CANDIDATE malachite=MALACHITE --cpu 0 \
  --bits 262144 524288 1048576 --cases div/balanced div/short render/decimal
```

The runner uses seed 987 for dense operands and shuffles the executable
order in every batch. It pins each invocation to one logical CPU, drops
the first batch, and reports the median elapsed time per operation of
seven measured batches by default. Operand files and parse input
preparation are outside the timed region. The runner checks that all
executables agree on the operation checksum and retains the individual
batch observations in the CSV. Correctness checks separately compare
complete quotients, remainders, and numeral strings.

The initial run, repeat, and ablation CSVs, exact executables, and
qualification logs are retained under the primary worktree's
`target/issue-987-block-evidence/`.
