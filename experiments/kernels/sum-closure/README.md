# Captured sum mapper (issue #1019)

`option.plenty` isolates the captured `map_or` mapper and the equivalent
explicit match. `option.c` performs the same checked subtraction and
comparison. The C function uses a status integer for its three outcomes.
`nearby.plenty` measures full dictionary scans using either implementation.
Both input patterns finish the scan. The second pattern repeats keys after
1,024 elements, outside the distance limit of 16.

From the repository root, with `plenty` pointing to the compiler under test:

```sh
plenty --dump clif-opt,asm experiments/kernels/sum-closure/option.plenty -o /tmp/sum-option
cc -O2 -c experiments/kernels/sum-closure/option.c -o /tmp/sum-option.o
objdump -d --no-show-raw-insn /tmp/sum-option.o
plenty --dump clif-opt,asm experiments/kernels/sum-closure/nearby.plenty -o /tmp/sum-nearby
PLENTY_CACHE=off plenty --compile experiments/kernels/sum-closure/nearby.plenty -o /tmp/sum-scan
perf stat -e cycles:u,instructions:u -- taskset -c 4 /tmp/sum-scan chain
perf stat -e cycles:u,instructions:u -- taskset -c 4 /tmp/sum-scan chain repeated
```

Use `match` in place of `chain` to run the explicit-match variant. All four
runs print `0`. Interleave runs from both compiler builds for timing.

## Isolated kernel

| x86-64 build | Absent instructions | Present instructions without overflow |
| --- | ---: | ---: |
| GCC 16.2.1, `-O2` | 4 | 9 |
| Plenty `e7050f63`, chained | 105 | 111 |
| Plenty `e7050f63`, match | 10 | 13 |
| Plenty with both fixes, either variant | 10 | 13 |

The original chained kernel used a 352-byte stack frame. It built a 48-byte
closure environment and repeatedly copied it via 16-byte loads and stores.
These copies, their null guards and address calculations, the scalar reloads,
and their frame setup account for the extra instructions. The scalar argument
form removes the environment and gives both variants the same 68-byte body,
down from 583 bytes for the chained form. The remaining differences from C
come from Plenty's frame pointer and two-register tagged return convention.

In the dictionary loop, the `Option` test selects the subtraction. The
`sub` and `jo` still check overflow. The comparison feeds `jle` directly
and the absent arm reaches insertion without a second tag test. The original
loop materialized the comparison as a boolean and tested the mapper's `Result`
before branching on it.
