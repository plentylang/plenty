# Bigint long division

Issue #987. The baseline is `320a3071`. Both builds use the same Plenty
compiler revision and optimization settings on Linux x86-64, with the candidate
compiler embedding the updated `std.bigint`. Malachite 0.14.0 is the
development comparator from the [bigint harness](../README.md).

Knuth division subtracts one trial quotient product per row. In base
`B = 2^32`, let `t = q*v + b + (B - 1 - u)`, where `u` is the current
numerator digit, `v` is the divisor digit, `q` is the trial quotient and
`b` is the incoming borrow. The desired difference `u - q*v - b` equals
`B - 1 - t`. The difference digit is the complement of the low 32 bits of
`t`, and the outgoing borrow is its high 32 bits. Every input is in
`0..B-1`, inclusive, so `t` is at most `B^2 - 1`. The high half keeps the
borrow in range for the next digit.

Form a bounded mutable slice once per row and traverse it with a counted
loop. The numerator scratch has `m + n + 1` digits, the divisor has `n`,
and the starting index `j` is in `0..m`, inclusive. The range `j:j+n`
therefore fits and leaves the high digit available for the Algorithm D
correction.
Pack the remainder from the original numerator scratch and skip scaling
passes when the factor is one.

## Row kernel

The C function takes the same lengths and retains the check for a shorter
divisor view. It has no `restrict` qualifier. Three paired
`perf stat -e instructions:u` runs at 128 and 256 repetitions subtract
setup and checksum overhead. All variants agree on the entire row checksum
and the outgoing borrow. The Plenty driver inlines its kernels. The C
probe keeps its function out of line so the generic loop can be inspected.

| Build | Standalone steady-state instructions per digit | Executable instructions per digit, median |
| --- | ---: | ---: |
| Plenty, offset and checked product | 41–42 | 41.54 |
| Plenty, bounded row view | 25–26 | 25.55 |
| Plenty, bounded row with complemented digits | 15 (17 if divisor view is shorter) | 14.04 |
| GCC 16.2.1, bounded row with complemented digits | 15 | 15.01 |

The offset loop's 43-instruction static union includes 18 instructions for
three repeated index/address calculations and their bounds and overflow checks.
The slice loop replaces them with two instructions for a divisor bounds check.
Both loops still have three product/borrow overflow branches, a five-instruction
borrow-selection diamond, three register moves and a separate loop back jump.
The complemented loop uses the bounded arithmetic described above. In its
15-instruction equal-length path the view-length test is hoisted, the loop
back edge is conditional, and two register moves remain. The C loop keeps
the divisor check inside the loop, folds the row load into XOR and forms
the low complement with a 32-bit `not`. These are source variants with the
same Cranelift configuration.

## Public API

Median user-space instructions per operation on an AMD Ryzen 7 7840HS, pinned
to CPU 0. Each row uses three paired instruction counts at N and 2N. For
division the size is the divisor width, and the numerator is the product of
two integers of that width. Render converts a number of the listed width
to decimal.

| Bits | Operation | Baseline | Candidate | Malachite | Candidate change |
| ---: | --- | ---: | ---: | ---: | ---: |
| 16 | Floor divmod | 9,630 | 9,630 | 139 | 0.00% |
| 16 | Decimal render | 13,338 | 13,338 | 535 | 0.00% |
| 256 | Floor divmod | 49,494 | 45,952 | 1,173 | −7.16% |
| 256 | Decimal render | 505,894 | 495,952 | 2,941 | −1.97% |
| 2,048 | Floor divmod | 404,440 | 289,288 | 11,689 | −28.47% |
| 2,048 | Decimal render | 3,863,736 | 3,727,223 | 28,128 | −3.53% |
| 8,192 | Floor divmod | 3,595,655 | 1,835,858 | 158,262 | −48.94% |
| 8,192 | Decimal render | 18,358,726 | 17,136,888 | 164,463 | −6.66% |
| 65,536 | Floor divmod | 180,928,174 | 69,600,764 | 4,538,273 | −61.53% |
| 65,536 | Decimal render | 323,899,231 | 264,958,532 | 4,438,322 | −18.20% |

Across 62 workloads, the largest instruction increase was 0.12% for
8,192-bit addition. The widest sample spread for any build/workload was
0.45%. Uncached optimized compilation of the benchmark used a median 834.64M
instructions with the baseline and 833.57M with the candidate. At 65,536 bits,
the remaining Malachite gaps are 15.34× for division and 59.70× for decimal
rendering. The experiment measures Linux x86-64 instruction cost.

The existing Malachite comparison matched all 16,378 lines from 195 signed
cases. Another 144 Python integer cases checked exact quotients and remainders
with 65- to 65,536-bit divisors. These cases combine all operand signs, low
and high leading digits, and zero, minimum nonzero and maximum remainders.
A regression in the integration suite covers the Algorithm D add-back path
with and without initial scaling, in both compiler modes.

## Reproduce

Use separate optimized release compiler binaries from baseline and candidate
checkouts. Set `evidence` to an absolute output directory and run from this
checkout. Set `candidate`, `baseline` and `malachite` to absolute paths to the
release compilers and the comparator binary, respectively.

```sh
mkdir -p "$evidence"
python3 -B experiments/bigint-comparison/compare.py generate --out "$evidence"
PLENTY_CACHE=off "$baseline" --no-strip --compile experiments/bigint-comparison/bench.plenty -o "$evidence/bench-baseline"
PLENTY_CACHE=off "$candidate" --no-strip --compile experiments/bigint-comparison/bench.plenty -o "$evidence/bench-candidate"
python3 -B experiments/bigint-comparison/algorithms-next/measure.py --quick \
  --baseline "$evidence/bench-baseline" --candidate "$evidence/bench-candidate" \
  --malachite "$malachite" --inputs "$evidence/benchmark-inputs.json" --out "$evidence/measure"

probe=experiments/bigint-comparison/division
PLENTY_CACHE=off "$candidate" --no-strip --compile "$probe/kernel.plenty" -o "$evidence/kernel-plenty"
PLENTY_CACHE=off "$candidate" --dump asm,clif-opt --dump-filter row "$probe/kernel.plenty" -o "$evidence/dump"
gcc -O2 -fno-ipa-cp -fno-tree-loop-distribute-patterns "$probe/kernel.c" -o "$evidence/kernel-c"
python3 experiments/kernels/loops.py < "$evidence/dump/asm.txt"
objdump -d --no-show-raw-insn "$evidence/kernel-c" | python3 experiments/kernels/loops.py
perf stat -e instructions:u "$evidence/kernel-plenty" complement 128
perf stat -e instructions:u "$evidence/kernel-plenty" complement 256
```

Repeat the paired commands for `kernel-plenty` with modes `offset` and
`view`, and for `kernel-c` with mode `complement`. Divide the difference in counts by `128 * 2048`.
The API harness does the same subtraction at calibrated iteration counts
and checks each operation's output checksum.
