# AVX scalar conversion experiment (issue #1015)

The bounded spectral-norm A and AT loops had one more instruction per pass
than the matched GCC 16.2.1 kernels at `x86-64-v3`. Cranelift cleared an XMM
register immediately before each `vcvtsi2sd` to provide its upper lane
without introducing a dependency on an earlier conversion.

The [local backend patch](backend.patch) applies to fork revision
`2986d9c5835ba6d03878312f8ef49703130bc65f`. The local fork commits are
`8471e3a4e88ab037173384b35c013b4bf2a8a627` for the lowering and
`8a6a6cbbf383ee1999202e16264ddf8b4568d903` for the regression update.
The lowering covers optimized AVX signed `i32` or `i64` to `f64` conversion
followed immediately by a division that uses the conversion only as its
denominator. The numerator must be a scalar constant or a lane of a vector
constant, defined in a different block that dominates the division. When
the numerator stays in a register across the loop, reuse avoids creating a
loop-carried dependency or keeping an extra register live. The division reads
only the conversion's low lane and copies its own upper lane from the numerator.
SSE2, unoptimized code, and conversions outside this pattern keep their
original lowering.

| A / AT inner pass on x86-64-v3 | Instructions |
|---|---:|
| GCC 16.2.1, matched C kernels | 12 / 12 |
| Plenty `ffce9762`, published fork pin | 13 / 13 |
| Native control, move the zero to the loop entry | 12 / 12 |
| Native control, reuse the already-loaded numerator | 12 / 12 |
| Plenty `ffce9762`, local Cranelift candidate | 12 / 12 |

The matched loops spend eight instructions on integer/address arithmetic,
index update, compare and branch, and four on conversion, division, multiply
with the input load folded in, and addition. The excess instruction in
Plenty is `vxorpd`. Both floating instruction order and the folded load are
preserved. The candidate adds no spills or reloads to either inner loop.

The two native controls changed only the two emitted matrix loops in an
initial benchmark binary. One moved the zeroing instruction before the
backedge target and supplied a separate zero register. The other reused the
numerator and kept the original loop start address. Each control reduced
elapsed time by about 5% at `n = 5500` on a Ryzen 7 7840HS, motivating the
compiler candidate.

In a separate loop with an indirect call, Plenty's pinned allocator spilled
the numerator. Reuse then put the reload on the path to the conversion
and division. The probe retired one fewer instruction per pass with
essentially unchanged median cycles on the 7840HS. Profitability with
spilled numerators remains unmeasured outside that probe.

## Measurements

For the compiler comparison, both release executables were built with
`--no-cache --cpu x86-64-v3`. The [runtime samples](runtime-samples.csv)
use ten pairs per size, alternating run order on CPU 3 of the 7840HS.
Within each input size, stdout matched byte for byte in all runs. The
table compares per-build medians.

| Input | Baseline elapsed | Candidate elapsed | Elapsed reduction | Cycle reduction | Instruction reduction |
|---|---:|---:|---:|---:|---:|
| `n = 2000` | 0.216 s | 0.211 s | 2.3% | 4.9% | 7.68% |
| `n = 5500` | 1.581 s | 1.518 s | 4.0% | 4.2% | 7.69% |

Elapsed times varied between runs. The median within-pair elapsed reductions
were 4.5% at `n = 2000` and 2.2% at `n = 5500`. The instruction counts
dropped by about 160 million and 1.21 billion respectively, one for each
inner-loop pass.

The [compile samples](compile-samples.csv) include six interleaved pairs per
workload, followed by twelve repeat pairs for each of the three program
builds. The synthetic AOT workload had median process elapsed times of
33.61 s for the baseline and 25.33 s for the candidate, with a combined range
of 21.50–68.51 s. The candidate's spectral-norm compile median was 39%
higher in the initial series and 2.4% lower on repeat.

| Program build (repeat) | Baseline elapsed | Candidate elapsed | CPU time change | Instruction change |
|---|---:|---:|---:|---:|
| Spectral norm | 243 ms | 237 ms | +0.4% | −0.015% |
| Fasta | 226 ms | 224 ms | +1.0% | −0.011% |
| N-body, `clist` | 308 ms | 307 ms | 0.0% | −0.009% |

Median retired instructions differed by less than 0.04% across all compile
workloads. These measurements include compiler startup and linking.

## Validation

The before-and-after compiler dumps matched byte for byte in 42 of 45
comparisons. The inputs were 13 benchmark programs and the two bounded
kernel sources, each built with `--dump-std` at optimized `x86-64`,
optimized `x86-64-v3`, and `x86-64-v3 --fast`. Only the three spectral-norm
sources at optimized `x86-64-v3` differed.

The native verifier passed 2,048 bit-exact matrix cases with protected span
boundaries, over-limit early returns, and matching floating exception flags
across all four rounding modes. The Cranelift regression checks emitted
instructions and execution with a nonzero upper lane and a loop-carried
numerator. In the shared-denominator case, the numerator remains hoisted and
exactly one conversion serves both consumers. A scratch build that allowed
multiple uses emitted two conversions and failed the revised check. The
original filetest passed with that scratch build.

All 212 Cranelift x64 filetests passed with registry regalloc2 0.15.2.
With Plenty's pinned allocator, the new test passed and 15 assembly
expectation failures had identical diagnostics before and after the patch.

Plenty's `just test --profile ci` used the offline Cranelift path override
and disabled the compiler cache. Across the suite and focused retries,
7,930 tests passed and nine were ignored. The retries used a writable
script cache and local socket access for tests blocked by the sandbox,
including tutorial examples. Docs CI passed the source-link check,
Python script tests, and complete mdBook 0.5.4 site check across 3,560 pages.

Runtime measurements cover one AVX host. Adopting the candidate in Plenty
requires publishing the Cranelift change and updating the compiler pin.

## Reproduction

Build the baseline compiler from Plenty `ffce9762`, which pins the Cranelift
revision above. Apply the patch with `git apply --unidiff-zero` in a clean
fork worktree at that revision. Build a second compiler with the temporary
[fork path override](../../../book/src/development/workflow.md#cranelift-and-regalloc2-forks)
and the same locked regalloc2. Use the two binaries separately with
`PLENTY_CACHE=off`.

```sh
gcc -O2 -march=x86-64-v3 -ffp-contract=off -fno-tree-loop-distribute-patterns \
  -c experiments/kernels/bounded-arguments/kernels.c -o /tmp/matrix-c.o
objdump -d --no-show-raw-insn /tmp/matrix-c.o | python3 experiments/kernels/loops.py

"$PLENTY" --no-cache --cpu x86-64-v3 --dump clif-opt,asm \
  experiments/kernels/bounded-arguments/kernels.plenty -o /tmp/matrix-dump
python3 experiments/kernels/loops.py -v < /tmp/matrix-dump/asm.txt
"$PLENTY" --no-cache --cpu x86-64-v3 --compile \
  benchmarks/game/spectral-norm/plenty/spectralnorm.plenty -o /tmp/spectral
taskset -c 3 perf stat -e instructions:u,cycles:u,task-clock:u /tmp/spectral 5500
```

The synthetic compile workload is the release `compile_bench` example
run as `compile_bench 1000 10 --workload aot`. Use the preceding
`--compile` flags for spectral norm, `fasta.plenty`, and
`nbody_clist.plenty` to measure end-to-end compilation.

For the native comparison, extract the two standalone matrix functions from
the optimized CLIF dump. Change their call convention to `system_v` for the
C entry points and replace the cold diagnostic call block with `trap user3`
as in the [load-folding verifier](../fp-load-folding/README.md#native-safety-checks).
Compile each pair with the corresponding `clif-util compile --target x86_64
--set has_avx --set opt_level=speed --set enable_llvm_abi_extensions=true
--set regalloc_checker=true -o output.o`. Export the four symbols named in
[`verify.c`](../fp-load-folding/verify.c), then link it against the two objects
with `gcc -O2 -frounding-math ... -lm`.
