# `std.bigint` — arbitrary-precision integers

`BigInt` is a signed integer with no fixed bit width. Import it from
`std.bigint`; constructors are `from_i64`, `from_u64`, and `parse`.
It is implemented directly in Plenty, with fallible ordinary list allocations.
There is no native dependency or engine.

```plenty
from std.bigint import from_i64, parse, BigIntError
from std.fmt import FormatError

def main() -> Result[(), AllocError | BigIntError | IoError | FormatError]:
    large = parse("18446744073709551616")?
    factor = from_i64(-7)
    print((large * factor)?)?
    print((large // factor)?, (large % factor)?)?
    print(large.to_string(16)?)?
    Ok(())
```
```output
-129127208515966861312
-2635249153387078803 -5
10000000000000000
```

## Construction and conversion

| API | Contract |
|---|---|
| `from_i64(value: i64) -> BigInt` | Inline value; no allocation. |
| `from_u64(value: u64) -> Result[BigInt, AllocError]` | Allocates only above `i64.MAX`. |
| `parse(text: &strview, radix: i64 = 10, max_digits: NonNegative[i64] = 10000) -> Result[BigInt, BigIntError]` | ASCII digits in radix 2–36, optional leading `+` or `-`; upper and lower case accepted. No whitespace, separators, or radix prefixes. The digit cap excludes the sign and includes leading zeros; checked before allocating arithmetic storage. |
| `BigInt.to_i64() -> Result[i64, BigIntError]` | Exact conversion; `OutOfRange` if it does not fit. |
| `BigInt.to_u64() -> Result[u64, BigIntError]` | Exact conversion; negative values and values above `u64.MAX` give `OutOfRange`. |
| `BigInt.to_f64() -> f64` | Nearest representable `f64`, ties to even; overflow gives signed infinity. Does not allocate. |
| `BigInt.bit_length() -> i64` | Number of bits in the absolute value; zero gives zero. Does not allocate. |
| `BigInt.to_string(radix: i64 = 10, max_digits: NonNegative[i64] = 10000) -> Result[str, BigIntError]` | Lower-case digits, no prefix or leading zeros; negative values start with `-`. The cap excludes that sign. |

`BigIntError` is `InvalidDigit`, `InvalidRadix`, `LimitExceeded`, `OutOfRange`,
`ZeroDivision`, or `Allocation(AllocError)`. Invalid text includes an empty
string or a sign without digits. Zero is always positive, including when parsed
from `-0`. Constructors and text conversions retain no input borrow.

## Arithmetic and ownership

Operators borrow both operands and return new owned values:

| Operation | Result |
|---|---|
| `a + b`, `a - b`, `a * b`, `-a` | `Result[BigInt, AllocError]` |
| `a // b`, `a % b` | `Result[BigInt, AllocError]`; zero divisor traps, naming the operation. |
| `n + a` where `n: i64` | `Result[BigInt, AllocError]` through `__radd__`. For `a + n`, write `a + from_i64(n)`. |
| `a << count`, `a >> count` where `count: i64` | `Result[BigInt, AllocError]`; negative counts trap. Right shift rounds toward negative infinity. |
| `a & b`, `a \| b`, `a ^ b` | `Result[BigInt, AllocError]`; infinite signed two's-complement semantics. |
| `a.checked_divmod(b)` | `Result[(BigInt, BigInt), BigIntError]`; zero gives `ZeroDivision`. |
| `a.add_assign(b)` | `Result[(), AllocError]`; `a` stays unchanged on failure. |
| `a.cmp(b)` | `i64`: `-1`, `0`, or `1`; no allocation. |

Division uses floor semantics: `a == q * b + r`, with `r` zero or having the
same sign as `b`, and `abs(r) < abs(b)`. There is no arithmetic overflow.
Allocation failure, including an unrepresentable capacity, reaches the caller.
An operator whose operands and result fit `i64` does not allocate. Results
crossing that boundary can allocate scratch even when the final value is small.
Use `(a + b)?` to propagate failure. In-place operator hooks cannot report
allocation failure, so `+=` is unsupported; use `add_assign` or explicit assignment.

`BigInt` is not a copy type. `copy(value)?` duplicates its limbs. Equality is
structural and exact because storage is canonical: values in the `i64` range
are inline; larger values use unsigned 64-bit magnitude limbs without high
zeros and a separate sign. The representation is private. Ordering operators
and dictionary/set keys are not supported; use `cmp` for ordering.

## Allocation, limits, and algorithms

Addition/subtraction allocate proportional to the operand lengths;
multiplication uses schoolbook arithmetic and balanced Karatsuba from 32 limbs.
Toom-3 handles medium products from 128 limbs when both operands have nonempty
thirds and their size ratio is at most 5:4. Its five evaluations are at 0, ±1,
2, and infinity. Large, balanced products can use exact five-prime
number-theoretic transforms (NTTs) with 128-bit digits (two `u64` limbs),
Garner CRT coefficient recovery, and base-2¹²⁸ carries. Five independently
generated, approximately 60-bit Proth primes provide exact convolution;
primality witnesses are retained in
`experiments/bigint-comparison/algorithms-next/constants.py`. CRT reconstructs
each coefficient in five scalar `u64` words. All residue, root and product
arithmetic uses exact Montgomery reduction with R = 2⁶⁴. The scalar radix-2
transforms reuse fixed scratch and computed twiddle tables. At 65,536 bits per
operand, the transform length is 1,024, compared with 8,192 for the former
16-bit path and 4,096 for the intermediate 32-bit path.
The direct NTT requires both operands to have at least 1,024 limbs, a size
ratio at most 2:1, and at most 524,288 combined limbs. The transform uses a
power-of-two root order and computes only the evaluations needed to recover
the packed product. The primes' 2⁴⁰ transform order exceeds the retained cap.
When the longer operand exceeds twice the shorter operand's width, qualifying
products split it into blocks the width of the shorter operand. Full blocks
share the shorter operand's evaluations and roots for each prime, then reuse
the inverse and partial-product buffers. Other shapes use Toom-3, Karatsuba,
or schoolbook multiplication as applicable, including values above the
transform's limb limit.
Karatsuba and Toom-3 borrow operand limb views and write
their low and high products (the evaluations at 0 and infinity) directly into
disjoint regions of the final output, avoiding two owned recursive product
arrays. Evaluation and interpolation scratch remains fallibly allocated.
The base case for multiword division uses Knuth's Algorithm D on the stored
64-bit limbs. It shifts the divisor so its top bit is set, then reuses one
reciprocal of the leading limb to estimate each quotient digit. It subtracts
the trial quotient product in place, computing the borrow bits with
word arithmetic. Whole zero limbs at the low end of the divisor can be
skipped, with the corresponding dividend limbs carried into the remainder.
Division by a single word retains its short-division path.
Large balanced divisions process quotient blocks recursively. Each block
divides by the high half of the divisor and uses a multiplication by the
omitted low half to correct its estimate. This gives subquadratic division
when both operand shapes can use subquadratic multiplication.
Sufficiently large divisions also use a Newton-built Barrett reciprocal
when the quotient blocks allow a short cyclic correction product.
The blocks reuse the transformed divisor and the transform roots. Full
blocks can likewise reuse the transformed reciprocal for their quotient
estimates. Preparing both operands costs scratch space proportional to the
divisor width, even when the quotient is several times wider. When the
quotient is narrower than the divisor and the cyclic correction fits, two
equal-width blocks can reuse a reciprocal sized to about half the quotient.
For divisors near and just above a power-of-two limb width, the correction
uses a cyclic transform at that width and a short product for the lowest
limbs. Together those residues determine the full correction. Newton
iteration also uses the cyclic product for qualifying residuals.
For a large divisor and a short quotient of `k` limbs, division can estimate
the quotient from the top `2k` dividend limbs and `k` divisor limbs, then
correct it with one product using the omitted divisor limbs. This path
requires a correction product that qualifies for direct or blocked
transform multiplication.

Parsing validates the full input before allocating limb storage. Leading
zeros count toward the digit cap and are skipped during construction.
Bases 2, 4, 8, 16, and 32 pack digit bits directly into limbs. Other bases
build a limb buffer by multiplying and adding groups of digits that fit in
one word. Large inputs split recursively and share the radix power at each
depth. Values in the signed 64-bit range are stored inline.

Non-power-of-two rendering also uses one split exponent per depth and
caches the normalized divisors. For qualifying repeated splits, it caches
a multi-limb Barrett reciprocal and the prepared transforms. It computes wider
reciprocals with Newton iteration. It corrects an estimate from the high
remainder limbs with a cyclic product of the quotient and divisor, with the
same short-product reconstruction when the divisor exceeds the transform
width. The cutoffs depend on the significant limb counts after removing
whole zero low limbs.

The renderer bounds the digit count from the bit length, compares against
one exact radix power if those bounds straddle the cap, and writes ASCII
digits into a shared buffer. A mutable copy of the magnitude holds remainders
in place. Sibling splits reuse slices of one quotient workspace, sized
from the digit widths. At the leaves, short division by a one-word
radix power extracts a grouped remainder. The renderer scales that
remainder to a fixed-point fraction and multiplies by the radix to extract
the digits. Each render computes its chunk divisor's reciprocal once.
Decimal chunks contain 19 digits. Bases 2, 4, 8, 16, and 32 extract bit groups
directly. Single-word rendering uses scalar division without a power cache.
Conversion scratch allocations are fallible. Large radix-power squares reuse
a single forward transform. General multiplication uses the two-operand transform.
The crossovers between implementations depend on operand size and shape.

Text conversion has explicit digit caps. Formatting through `display[W](out:
&mut Formatter[W]) -> Result[(), FormatError]` uses decimal with the default
10,000-digit cap; exhaustion becomes `FormatError.LimitExceeded`, and allocation
failure becomes `FormatError.Allocation`. Larger intentional output uses
`to_string(max_digits=...)`. Formatter output has its own byte limit.

Arithmetic is bounded by the existing operand sizes and requested shift count,
rather than a global precision limit. Callers processing untrusted arithmetic
must enforce their own operation and bit budgets; repeated multiplication or a
huge shift can request large memory. A zero left shift operand returns zero
without allocating, even for a huge count.

The compiler provides two allocation-free `u64` limb methods:
`a.widening_mul(b) -> (u64, u64)` returns the low and high halves of the exact
128-bit product; `a.carrying_add(b, carry: bool) -> (u64, bool)` returns the
wrapped sum and carry-out for `a + b + carry`. The required boolean here is a
numeric carry bit, not a mode selection.

## Acknowledgements

- Donald Knuth, *The Art of Computer Programming*, volume 2,
  [Seminumerical Algorithms](https://www-cs-faculty.stanford.edu/~knuth/taocp.html),
  section 4.3.1: schoolbook arithmetic and normalized long division (Algorithm D).
- Donald Knuth, *The Art of Computer Programming*, volume 2,
  [Seminumerical Algorithms](https://www-cs-faculty.stanford.edu/~knuth/taocp.html),
  section 4.3.3: Karatsuba's three-product and Toom-3's five-evaluation
  multiplication algorithms. These are
  direct Plenty implementations of published algorithms; no third-party code
  or data ships.
- [FLINT: Multiplication with several primes and CRT](https://flintlib.org/doc/algorithms/fft/multimodular.html):
  the published multimodular convolution method used for large multiplication,
  also used by Malachite’s FLINT-derived small-prime FFT. This is an independent
  integer radix-2 implementation; no FLINT/Malachite code or tables ship.
- Joris van der Hoeven, [Truncated FFTs](https://www.texmacs.org/joris/issac04/issac04.html),
  describes how to recover a linear convolution from a truncated transform.
  Plenty applies the algorithm over its exact prime fields.
- Peter L. Montgomery, [Modular multiplication without trial division](https://doi.org/10.1090/S0025-5718-1985-0777282-X):
  exact modular arithmetic for transform residues, roots and products.
- [Malachite 0.14.0](https://docs.rs/crate/malachite-nz/0.14.0) informed
  bit-length sizing, direct bit packing and extraction, fractional digit
  extraction, recursive rendering scratch reuse, radix-power squaring,
  short-quotient division, and Barrett
  division with Newton reciprocals, cyclic corrections and equal-width blocks.
  This is an independent Plenty implementation.
