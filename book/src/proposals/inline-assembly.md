# Trusted inline assembly

**Proposed.** Current native code access is through
[trusted C declarations](../design/facilities/04-interop/trusted-declarations.md)
and [compiler intrinsics](../design/comparisons/rust.md#where-unchecked-code-lives).
B99 in the [backlog](../backlog.md#safety-boundaries) tracks the investigation.

## When inline assembly earns its place

Inline assembly would let a trusted binding place a short instruction sequence
inside a Plenty function. Its typed operands would participate in register
allocation with the surrounding code. This is useful when the sequence must
execute in the caller's machine context or when a call boundary dominates the
cost of a small operation on a measured hot path.

An assembly routine with a C-compatible signature already works through
`extern def` in a `.plentyi` module. A local x86-64 smoke test assembled a
`bswap` routine, linked its object with `--link-arg`, and returned the expected
byte-swapped `u64` to Plenty. That establishes a baseline for calling native
code. For a recurring operation with well-defined semantics, a typed intrinsic
can also lower directly to machine instructions while exposing more information
to the optimizer. Start an inline-assembly prototype when a concrete caller
needs the in-function placement and a dedicated intrinsic would be a poor fit.
[Native linking](../design/toolchain/01-building/linking.md)

## Prototype contract

Use a private declaration in a trusted `.plentyi` module on x86-64 GNU/Linux.
Ordinary Plenty code can call a typed wrapper. A target-specific binding can
choose a portable implementation with [`if const target.arch`](../design/language/02-source/target-conditionals.md).
Diagnose a selected declaration on an unsupported target at its source span.

The following sketch uses `bswap` to illustrate a tied operand. The syntax is
provisional, and byte swapping serves as an encoding and execution fixture.

```text
asm def swap_word(value: u64) -> u64:
    inout("rax", value)
    "bswap rax"
```

- Operands are `i64` or `u64` values in explicitly named 64-bit general-purpose
  registers. Accept only a small target-defined set of caller-saved registers
  available to the allocator. An input is read-only unless declared `inout`.
  A declaration returns unit or one scalar, supplied by an `out` register or
  by the final value of an `inout` register. Separate outputs and scratch
  clobbers must be disjoint from inputs. An `inout` occupies the same register
  on entry and exit, even if the original input remains live after the block.
- Every additional register written by the sequence is an explicit clobber.
  Assume arithmetic flags are clobbered for every block. Reject aliases,
  duplicate or conflicting constraints, and registers outside the supported
  set. The compiler models the declared writes and must keep live values out
  of those registers or preserve them across the block.
- The assembly is a compile-time Intel-syntax string containing a straight-line
  sequence of baseline instructions with one fallthrough exit. The author must
  depend only on declared inputs, leave the stack and undeclared machine state
  intact, define the output, and avoid memory access, calls, branches,
  privileged instructions, and instructions
  that can trap for admitted inputs. The prototype admits no pointer, borrow,
  aggregate, or integer domain operands. The assembler validates encoding and
  the compiler checks the declared interface. The instruction-level contract
  remains a trust obligation on the author, as with native code behind FFI.
- Classify the operation as a distinct trusted native effect. Reject it when
  reachable from workers, certified host callbacks, or actor stages in the
  prototype, including through wrappers and destructors. Report the path to the
  declaration. This leaves concurrency admission to a separate design decision.
  [Native effects](../design/facilities/03-concurrency/effects.md) ·
  [Proposed actor recovery certification](trusted-safety-boundaries/recovery.md)

## Lowering and assembly

The pinned Cranelift revision (`2986d9c5`) has no inline-assembly IR operation.
Its machine operand visitor already has fixed uses, fixed definitions, tied
definitions and clobber sets. Prototype against the local fork and verify that
its current allocator can express the constraints before considering allocator
changes. [Cranelift and regalloc2 fork workflow](../development/workflow.md#cranelift-and-regalloc2-forks)

1. Check signatures, operand constraints, template form, target and effects in
   the frontend. `--check` performs these source checks without running native
   tools. Report assembler failures during emission at the template span.
2. Assemble the template during native emission with the target assembler and
   extract its code bytes from the resulting object. Fixed registers make
   encoding independent of register allocation. Give each template one
   compiler-generated section and reject labels and directives before assembly
   and relocations after assembly. Cap the byte length and account for it in
   machine layout. Require the assembler only when emitting an assembly-bearing
   program, including when producing a standalone object.
3. Add an opaque, side-effecting IR operation carrying the bytes, typed
   operands, and clobbers. On every path that reaches it, execute it in order
   even if the output is unused. The optimizer cannot speculate its execution
   or reuse a prior result in place of executing it. Lower it to an x86-64
   machine instruction that presents those constraints to register allocation
   and emits the sequence through Cranelift's machine buffer after allocation.
   The surrounding Plenty function still owns the
   frame, unwind information, and control flow. [Native emission](../design/toolchain/02-architecture/emission.md)
4. For the prototype, bypass caches that could replay assembly-bearing native
   artifacts without checking the encoder and its inputs. Before enabling
   cache reuse, cover the template, operands, target and assembler identity
   in any cache key that can return an artifact before assembly. Include the
   assembled bytes and constraints in the function codegen key.
   [Compilation caches](../design/toolchain/02-architecture/cache.md)

## Evidence for adoption

- Use fork filetests to prove allocation with inputs that remain live, tied
  outputs, and live values across clobbers under spill pressure. Exercise both
  optimized and fast-compile builds. Check an unused-output case and a guarded
  block with disassembly to demonstrate retention and placement. Use Plenty
  integration tests to prove results, diagnostics, indirect effect reachability,
  and a target-conditional fallback. [Fork filetests](../development/fork-filetests.md)
- Check that the expected bytes land in the caller with no call boundary and
  survive final object emission. Compare a real caller with the existing FFI
  route and a typed intrinsic where applicable. For a speed claim, compare
  elapsed time on the real workload and explain extra moves, spills or calls
  with disassembly. [Emitted-code workflow](../development/emitted-code.md)
- Qualify AArch64 separately. Register classes selected by the allocator would
  require template substitution after allocation. Memory operands would need a
  contract for borrowing, aliasing and ordering. CPU extensions beyond the
  baseline would need checks against the selected target features. Each is a
  separate decision once a concrete use case warrants it.

Rust's `asm!` provides precedent for treating the template as a black box with
declared operands and effects. Its larger interface includes register classes,
memory effects and control-flow operands. The bounded prototype tests the part
Plenty needs first: preserving the boundary between assembly and the compiler's
own instructions. [Rust inline assembly reference](https://doc.rust-lang.org/reference/inline-assembly.html)
