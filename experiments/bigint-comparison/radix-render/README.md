# Bigint radix rendering (issue #987)

The baseline is the local merge of PR #1401 (`858ff7b9`). Both Plenty
executables use the same release compiler revision and optimization settings.
The candidate embeds the changed standard library. Malachite is the locked
development comparator from the [bigint harness](../README.md).

## Malachite's conversion path

The benchmark calls `Integer::to_string_base(10)`. In the pinned
[malachite-nz 0.14.0 crate](https://docs.rs/crate/malachite-nz/0.14.0), this goes
through `src/integer/conversion/string/to_string.rs` to the limb implementation
of descending digits in `src/natural/conversion/digits/general_digits.rs`.
The relevant 64-bit thresholds are in `src/platform_64.rs`.

- Power-of-two bases extract bit groups. Other small bases allocate a digit
  array from a fixed-point estimate of bit length divided by the radix's
  logarithm. Leading padding is removed after conversion.
- Below 43 limbs, rendering uses a grouped short-division basecase. At
  recursive leaves the threshold is 18 limbs. One decimal division extracts
  a remainder modulo 10¹⁹. A preinverted two-by-one division generates each
  such remainder, and multiply-high operations extract its digits.
- Larger conversions split through a table of radix powers, excluding any
  zero low limbs in the divisor. The renderer reuses scratch and precomputes
  approximate inverses for power rows with at least 50 significant limbs.
  Reusing these Barrett inverses moves most of the division work into
  multiplication and amortizes the cost of computing each inverse.
- In `src/natural/arithmetic/div_mod.rs`, ordinary division switches among
  normalized schoolbook, divide-and-conquer and Barrett division based on
  operand shape. The 64-bit thresholds are 61 limbs for divide-and-conquer
  and 809 for the balanced Barrett path. The 65,536-bit product/divisor
  fixture reaches Barrett. Parsing uses groups of digits in the basecase
  and combines them through a power table for larger inputs.

## Plenty implementation

Powers of two are rendered directly from limb bit groups, including groups
that straddle limb boundaries. For other radices, let `r^k` be the largest
power that fits in a base-2³² digit and `L = floor(log2(r^k))`. A nonzero
magnitude with bit length `b` needs at least
`floor((b-1)*k/(L+1))+1` and at most `ceil(b*k/L)` radix digits. These
follow from `2^(b-1) <= n < 2^b` and `2^L <= r^k < 2^(L+1)`. The
bounds handle most cap checks by themselves. When the cap falls between
them, one comparison with `r^cap` determines whether the output fits.
The sign sits outside the magnitude cap.

Recursive splits fill disjoint regions of one ASCII buffer. A leaf extracts
`k` digits at a time using one in-place pass over base-2³² scratch digits,
then handles the final one or two scratch digits with scalar arithmetic. In
each pass, the incoming remainder is below the divisor, so combining it
with another scratch digit fits in a `u64`. The quotient fits one scratch
digit. With a divisor below 2³², a pass removes at most one leading scratch
digit. The decimal chunk here is 10⁹. Each scratch digit uses hardware
division, so reciprocal division remains a separate change.

A prototype cutoff sweep through 4, 8, 16, 32, 48, 64, 96 and 128 limbs
tested the grouped leaf before building the final embedded library. At
65,536 bits, 8, 32, 64, 96 and 128 limbs took about 83.3M, 74.1M, 70.3M,
67.0M and 66.7M instructions respectively. At 8,192 bits, 96 limbs had a
median of 0.194 ms across five batches and 128 limbs took 0.209 ms. The
96-limb cutoff keeps the grouped leaf where hardware-division throughput
helped elapsed time in this sweep. Crossover values are host-specific.

## Short-division kernel

The [Plenty loop](kernel.plenty) is extracted from production. The
[matching C loop](kernel.c) uses a runtime divisor and a writable pointer
with no `restrict` qualifier. Full-row checksums agree for both batch
sizes. The Plenty loop executes 18 instructions in its standalone assembly.
GCC 16.2.1 uses nine. Relative to GCC, Plenty adds two instructions for
bounds, two for the zero-divisor check, one for the indexed address, one
for the separate load, two for reconstructing the remainder, and a
register move. GCC takes quotient and remainder from one `div`. Three
128/256-row instruction pairs give 17.01 instructions per scratch digit
in the inlined Plenty executable and 9.01 in C.

## Measurements

Median user-space instructions per operation on an AMD Ryzen 7 7840HS
running Linux x86-64. Three paired N/2N batches pinned to CPU 0 subtract
the cost of fixed setup. The same operands and cap are used by the two
Plenty builds.

| Bits | Decimal render baseline | Candidate | Malachite | Instruction reduction |
| ---: | ---: | ---: | ---: | ---: |
| 16 | 13,338 | 4,489 | 535 | 66.3% |
| 256 | 496,157 | 60,190 | 2,940 | 87.9% |
| 2,048 | 3,725,378 | 459,648 | 28,134 | 87.7% |
| 8,192 | 17,135,490 | 2,998,081 | 164,536 | 82.5% |
| 65,536 | 264,909,652 | 67,054,004 | 4,438,172 | 74.7% |

The largest instruction increase outside rendering was 0.17% for 256-bit
addition across 57 controls. All 62 public workload checksums agree, and
the largest spread among the three samples was 0.50%. Uncached optimized
compilation of the benchmark rose from 833.50M to 866.27M instructions
(3.9%) across three runs per build. A separate radix
probe at 65,536 bits measured 271.7M to 14.9M instructions for base 8,
305.8M to 11.1M for base 16, and 325.2M to 63.5M for base 36.

At 65,536 bits, decimal rendering still uses 15.1× Malachite's instructions.
Divmod on the product/divisor control uses 69.6M instructions, 15.3×
Malachite. The large radix splits still rely on quadratic division. One
follow-up is to measure multiplication-based division with Plenty's
existing large multiplication, then reuse reciprocals for equally sized
radix splits. Grouped input parsing and a preinverted leaf also remain
possible improvements.

Validation compared 16,378 outputs from 195 Malachite cases across radices
2 through 36. A Python oracle also matched 47 render cases, including
random inputs in every radix at 7,000 bits and negative random and sparse
inputs at 65,536 bits. The integration suite checks exact and one-short
digit caps on both sides of radix powers and around the recursive cutoff
in both compiler modes.

## Reproduce

Choose an absolute evidence directory and paths to both release compilers
and the locked Malachite executable. Run from this checkout with
`PLENTY_CACHE=off`.

```sh
mkdir -p "$evidence"
export PLENTY_CACHE=off
python3 -B experiments/bigint-comparison/compare.py generate --out "$evidence"
bench=experiments/bigint-comparison/bench.plenty
"$baseline" --no-strip --compile "$bench" -o "$evidence/bench-baseline"
"$candidate" --no-strip --compile "$bench" -o "$evidence/bench-candidate"
python3 -B experiments/bigint-comparison/algorithms-next/measure.py --quick \
  --baseline "$evidence/bench-baseline" --candidate "$evidence/bench-candidate" \
  --malachite "$malachite" --inputs "$evidence/benchmark-inputs.json" --out "$evidence/measure"
"$candidate" --compile "$evidence/oracle.plenty" -o "$evidence/oracle"
python3 -B experiments/bigint-comparison/compare.py compare --out "$evidence" --malachite "$malachite"

probe=experiments/bigint-comparison/radix-render
"$candidate" --compile "$probe/bench.plenty" -o "$evidence/radix-candidate"
"$baseline" --compile "$probe/bench.plenty" -o "$evidence/radix-baseline"
"$candidate" --no-strip --compile "$probe/kernel.plenty" -o "$evidence/kernel-plenty"
"$candidate" --dump asm,clif-opt --dump-filter divide_render_chunk "$probe/kernel.plenty" -o "$evidence/dump"
gcc -O2 -fno-ipa-cp "$probe/kernel.c" -o "$evidence/kernel-c"
python3 experiments/kernels/loops.py < "$evidence/dump/asm.txt"
objdump -d --no-show-raw-insn --disassemble=divide_render_chunk "$evidence/kernel-c" | python3 experiments/kernels/loops.py
perf stat -e instructions:u "$evidence/kernel-plenty" 128 1000000000
perf stat -e instructions:u "$evidence/kernel-plenty" 256 1000000000
```

For the separate radix probe, pass the 65,536-bit `a` from
`benchmark-inputs.json` as a hexadecimal argument, followed by the radix
and repetition count. A zero repetition count prints the full result.
For each engine, measure four and eight repetitions and divide the
instruction difference by four. For the kernel use a 128/256 repetition
pair per engine and divide by 128 × 2,048 digits.
