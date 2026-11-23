//! BigInt is implemented in Plenty; fixed-width Rust arithmetic is an independent oracle.
use std::process::Command;

fn run(body: &str, optimize: bool) -> String {
    let source = format!(
        "from std.bigint import BigInt, BigIntError, from_i64, from_u64, parse\nfrom std.fmt import FormatError\ndef main() -> Result[(), AllocError | BigIntError | IoError | FormatError]:\n{body}    Ok(())\n"
    );
    run_program(&source, optimize)
}

fn run_program(source: &str, optimize: bool) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.plenty");
    std::fs::write(&path, source).unwrap();
    let exe = dir.path().join("program");
    plenty::compile_file_to_executable_with_options(
        &path,
        &exe,
        None,
        &plenty::CompileOptions {
            optimize,
            ..Default::default()
        },
    )
    .unwrap();
    let output = Command::new(exe).output().unwrap();
    assert!(
        output.status.success(),
        "status={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn arithmetic_matches_fixed_width_oracle() {
    let mut body = String::new();
    let mut expected = String::new();
    let mut state = 0x123456789abcdef0u64;
    for i in 0..32 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let a = (i128::from(state) << 58) | i128::from(state.rotate_left(19));
        let a = if i % 2 == 0 { a } else { -a };
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let b = i128::from(state | 1);
        let b = if i % 3 == 0 { -b } else { b };
        body.push_str(&format!("    a{i} = parse(\"{a}\")?\n    b{i} = parse(\"{b}\")?\n    print((a{i} + b{i})?, (a{i} - b{i})?, (a{i} // b{i})?, (a{i} % b{i})?, (a{i} & b{i})?, (a{i} | b{i})?, (a{i} ^ b{i})?, (a{i} >> 67)?)?\n"));
        let mut q = a / b;
        let mut r = a % b;
        if r != 0 && ((a < 0) != (b < 0)) {
            q -= 1;
            r += b;
        }
        expected.push_str(&format!(
            "{} {} {q} {r} {} {} {} {}\n",
            a + b,
            a - b,
            a & b,
            a | b,
            a ^ b,
            a >> 67
        ));
    }
    assert_eq!(run(&body, false), expected);
    assert_eq!(run(&body, true), expected);
}

#[test]
fn boundaries_canonicalization_and_limits() {
    let body = r#"    a = from_i64(i64.MIN)
    b = (-a)?
    c = (b - from_i64(1))?
    print(a, b, c, str.repr(b.to_i64())?, from_u64(u64.MAX)?.to_u64()?)?
    print(c == from_i64(i64.MAX), (b + a)? == from_i64(0), parse("-000")? == from_i64(0))?
    print(str.repr(parse(""))?, str.repr(parse("+"))?, str.repr(parse(" 1"))?, str.repr(parse("0x10", 16))?, str.repr(parse("2", 2))?, str.repr(parse("1", 1))?)?
    print(str.repr(parse("123", max_digits=2))?, str.repr(parse("0", max_digits=0))?, str.repr(from_i64(0).to_string(max_digits=0))?, str.repr(from_i64(99).to_string(max_digits=1))?)?
    print(parse("Z", 36)?.to_string(36)?, str.repr(from_i64(-1).checked_divmod(&from_i64(0)))?)?
    print((from_i64(-1) >> 999)?, (from_i64(1) >> 999)?, (from_i64(0) << i64.MAX)?)?
    print((a // from_i64(-1))? == b, (a % from_i64(-1))? == from_i64(0), b.bit_length(), a.bit_length())?
"#;
    let expected = "-9223372036854775808 9223372036854775808 9223372036854775807 Result[i64, BigIntError].Err(BigIntError.OutOfRange) 18446744073709551615\nTrue True True\nResult[BigInt, BigIntError].Err(BigIntError.InvalidDigit) Result[BigInt, BigIntError].Err(BigIntError.InvalidDigit) Result[BigInt, BigIntError].Err(BigIntError.InvalidDigit) Result[BigInt, BigIntError].Err(BigIntError.InvalidDigit) Result[BigInt, BigIntError].Err(BigIntError.InvalidDigit) Result[BigInt, BigIntError].Err(BigIntError.InvalidRadix)\nResult[BigInt, BigIntError].Err(BigIntError.LimitExceeded) Result[BigInt, BigIntError].Err(BigIntError.LimitExceeded) Result[str, BigIntError].Err(BigIntError.LimitExceeded) Result[str, BigIntError].Err(BigIntError.LimitExceeded)\nz Result[(BigInt, BigInt), BigIntError].Err(BigIntError.ZeroDivision)\n-1 0 0\nTrue True 64 64\n";
    let expected = expected
        .replace("(BigInt, BigInt)", "tuple[BigInt, BigInt]")
        .replace("BigInt", "std.bigint.BigInt");
    assert_eq!(run(body, false), expected);
}

#[test]
fn limb_primitives_match_u128() {
    let mut body = String::new();
    let mut expected = String::new();
    for (a, b, carry) in [
        (0u64, 0u64, false),
        (u64::MAX, u64::MAX, true),
        (1, u64::MAX, false),
        (1, u64::MAX, true),
        (0xdeadbeef01234567, 0xfedcba9876543210, true),
    ] {
        body.push_str(&format!(
            "    print({a}u64.widening_mul({b}u64), {a}u64.carrying_add({b}u64, {}))?\n",
            if carry { "True" } else { "False" }
        ));
        let product = u128::from(a) * u128::from(b);
        let sum = u128::from(a) + u128::from(b) + u128::from(carry);
        expected.push_str(&format!(
            "({}, {}) ({}, {})\n",
            product as u64,
            product >> 64,
            sum as u64,
            if sum >> 64 != 0 { "True" } else { "False" }
        ));
    }
    assert_eq!(run(&body, false), expected);
    assert_eq!(run(&body, true), expected);
}

#[test]
fn every_radix_round_trips_and_float_rounding_is_exact() {
    let n = u128::MAX;
    let mut body = String::new();
    let mut expected = String::new();
    for radix in 2..=36u128 {
        let mut value = n;
        let mut digits = Vec::new();
        while value > 0 {
            digits.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(value % radix) as usize] as char);
            value /= radix;
        }
        let text: String = digits.into_iter().rev().collect();
        body.push_str(&format!(
            "    print(parse(\"{text}\", {radix})?.to_string({radix})?)?\n"
        ));
        expected.push_str(&format!("{text}\n"));
    }
    for n in [
        9007199254740991i128,
        9007199254740993,
        9007199254740995,
        (1i128 << 120) + (1i128 << 66) + 1,
        -((1i128 << 120) + (1i128 << 66) + 1),
        i128::MAX,
        i128::MIN,
    ] {
        body.push_str(&format!(
            "    print(parse(\"{n}\")?.to_f64() == {:.17e})?\n",
            n as f64
        ));
        expected.push_str("True\n");
    }
    body.push_str("    print((from_i64(1) << 1024)?.to_f64() == f64.INF, (from_i64(-1) << 1024)?.to_f64() == -f64.INF)?\n");
    expected.push_str("True True\n");
    assert_eq!(run(&body, false), expected);
    assert_eq!(run(&body, true), expected);
}

#[test]
fn radix_parsing_preserves_word_boundaries_and_leading_zero_caps() {
    let mut body = String::new();
    let mut expected = String::new();
    for radix in [2u128, 3, 4, 8, 10, 12, 16, 24, 32, 36] {
        for n in [
            1 << 63,
            u128::from(u64::MAX),
            1 << 64,
            (1 << 65) + 3,
            u128::MAX,
        ] {
            let mut remaining = n;
            let mut reversed = Vec::new();
            while remaining != 0 {
                let digit = b"0123456789abcdefghijklmnopqrstuvwxyz"[(remaining % radix) as usize];
                reversed.push(if reversed.len() % 2 == 0 {
                    digit.to_ascii_uppercase()
                } else {
                    digit
                });
                remaining /= radix;
            }
            reversed.reverse();
            let digits = String::from_utf8(reversed).unwrap();
            let width = digits.len() + 3;
            body.push_str(&format!(
                concat!(
                    r#"    print(parse("+000{digits}", {radix}, {width})?, parse("-000{digits}", {radix}, {width})?)?"#,
                    "\n"
                ),
                digits = digits,
                radix = radix,
                width = width,
            ));
            expected.push_str(&format!("{n} -{n}\n"));
        }
    }
    body.push_str(
        r#"    match parse("-000001", 2, 5):
        case Err(BigIntError.LimitExceeded):
            print("limit")?
        case _:
            print("wrong result")?
    match parse("+00000ａ", 16, 16):
        case Err(BigIntError.InvalidDigit):
            print("invalid digit")?
        case _:
            print("wrong result")?
    print(parse("-0000000000000000000000000000", 3)? == from_i64(0))?
"#,
    );
    expected.push_str("limit\ninvalid digit\nTrue\n");
    assert_eq!(run(&body, false), expected);
    assert_eq!(run(&body, true), expected);
}

#[test]
fn large_radix_parsing_matches_independent_power_expansion() {
    let body = r#"    # 36^120001 - 1 has 120001 digits, including mixed letter case.
    text = ("-" + ("Zz".repeat(60000)? + "Z")?)?
    parsed = parse(text, 36, 120001)?
    # Form the expected value from the public multiplication operator,
    # which does not use the parser's radix powers or squaring path.
    mut power = from_i64(36)
    mut expected = from_i64(1)
    mut exponent = 120001
    while exponent > 0:
        if exponent & 1 != 0:
            expected = (expected * power)?
        exponent = exponent // 2
        if exponent != 0:
            power = (power * power)?
    print((parsed + expected)? == from_i64(1))?
"#;
    assert_eq!(run(body, false), "True\n");
    assert_eq!(run(body, true), "True\n");
}

#[test]
fn radix_rendering_preserves_exact_caps_and_internal_zeroes() {
    let mut source = String::from(
        r#"from std.bigint import BigIntError, parse

def check(text: str, radix: i64, width: NonNegative[i64], smaller: NonNegative[i64]) -> Result[(), BigIntError | IoError]:
    value = parse(text, radix)?
    exact = value.to_string(radix, width)?
    too_small = match value.to_string(radix, smaller):
        case Err(BigIntError.LimitExceeded):
            True
        case _:
            False
    print(exact == text, too_small)?
    Ok(())

def main() -> Result[(), BigIntError | IoError]:
"#,
    );
    let mut count = 0;
    for (radix, exponent) in (2..=36).map(|radix| (radix, 70)).chain([
        (10, 19),
        (10, 20),
        (10, 38),
        (10, 1849),
        (10, 1850),
        (10, 1900),
        (10, 4096),
    ]) {
        let last_digit = b"0123456789abcdefghijklmnopqrstuvwxyz"[radix - 1] as char;
        let before = last_digit.to_string().repeat(exponent);
        let power = format!("1{}", "0".repeat(exponent));
        let after = format!("-1{}1", "0".repeat(exponent - 1));
        for (text, width) in [
            (before, exponent),
            (power, exponent + 1),
            (after, exponent + 1),
        ] {
            source.push_str(&format!(
                "    check(\"{text}\", {radix}, {width}, {})?\n",
                width - 1,
            ));
            count += 1;
        }
    }
    source.push_str("    Ok(())\n");
    let expected = "True True\n".repeat(count);
    assert_eq!(run_program(&source, false), expected);
    assert_eq!(run_program(&source, true), expected);
}

#[test]
fn wide_division_corrects_estimates_and_preserves_low_remainder() {
    let source = r#"from std.bigint import BigInt, BigIntError, from_i64

def check(a: &BigInt, b: &BigInt, quotient: &BigInt, remainder: &BigInt) -> Result[(), AllocError | BigIntError | IoError]:
    negative_a = (-a)?
    negative_b = (-b)?
    negative_q = if remainder.is_zero():
        (-quotient)?
    else:
        ((-quotient)? - from_i64(1))?
    adjusted_r = if remainder.is_zero():
        from_i64(0)
    else:
        (b - remainder)?
    (q_pos, r_pos) = a.checked_divmod(b)?
    (q_an, r_an) = negative_a.checked_divmod(b)?
    (q_bn, r_bn) = a.checked_divmod(&negative_b)?
    (q_both, r_both) = negative_a.checked_divmod(&negative_b)?
    print(q_pos == quotient and r_pos == remainder,
          q_an == negative_q and r_an == adjusted_r,
          q_bn == negative_q and r_bn == (-adjusted_r)?,
          q_both == quotient and r_both == (-remainder)?)?
    Ok(())

def main() -> Result[(), AllocError | BigIntError | IoError]:
    one = from_i64(1)
    word = (one << 64)?
    upper = (one << 511)?
    divisor = (upper + one)?
    # The two leading divisor limbs admit a quotient one too high.
    # The low limb forces the final add-back.
    below_twice = ((divisor << 1)? - one)?
    check(&below_twice, &divisor, &one, &upper)?
    # The highest numerator limb equals the highest divisor limb in
    # the final row, saturating the trial quotient at 2^64 - 1.
    max_word = (word - one)?
    below_shift = ((divisor << 64)? - one)?
    check(&below_shift, &divisor, &max_word, &upper)?
    # One limb below the top changes the preliminary trial correction.
    next = (max_word << 384)?
    trial_divisor = (divisor + next)?
    trial_remainder = ((trial_divisor - (one << 384)?)? - one)?
    trial_numerator = (trial_divisor + trial_remainder)?
    check(&trial_numerator, &trial_divisor, &one, &trial_remainder)?
    # Renormalize an unaligned divisor and retain two untouched low limbs.
    trimmed = ((one << 520)? + one)?
    shifted_divisor = (trimmed << 128)?
    low_remainder = ((one << 128)? - one)?
    shifted_a = ((shifted_divisor << 64)? + low_remainder)?
    check(&shifted_a, &shifted_divisor, &word, &low_remainder)?
    # A multiword divisor can have just one significant limb after trimming.
    square_word = (word << 64)?
    check(&square_word, &word, &word, &from_i64(0))?
    Ok(())
"#;
    let expected = "True True True True\n".repeat(5);
    assert_eq!(run_program(source, false), expected);
    assert_eq!(run_program(source, true), expected);
}

#[test]
fn recursive_division_handles_a_saturated_quotient_block() {
    let body = r#"    one = from_i64(1)
    divisor = ((one << 131071)? + one)?
    block = (one << 65536)?
    # The initial remainder is one below the divisor. Its high half
    # matches the divisor's, so the recursive trial block saturates.
    a = (((divisor << 65536)?) - one)?
    (q, r) = a.checked_divmod(&divisor)?
    print(q == (block - one)?, r == (divisor - one)?)?
    negative = (-a)?
    (floor, modulus) = negative.checked_divmod(&divisor)?
    print(floor == (-block)?, modulus == one)?
"#;
    assert_eq!(run(body, false), "True True\nTrue True\n");
    assert_eq!(run(body, true), "True True\nTrue True\n");
}

#[test]
fn short_quotient_division_corrects_the_truncated_estimate() {
    let body = r#"    one = from_i64(1)
    divisor = ((one << 262143)? + ((one << 196608)? - one)?)?
    block = (one << 65536)?
    shifted = (divisor << 65536)?
    # The leading limbs estimate block - 2, two more than the true quotient.
    three_divisors = ((divisor << 1)? + divisor)?
    a = ((shifted - three_divisors)? - one)?
    (q, r) = a.checked_divmod(&divisor)?
    remainder = (divisor - one)?
    print(q == (block - from_i64(4))?, r == remainder)?
    # The truncated estimate saturates when the leading prefixes are equal.
    (max_q, max_r) = (shifted - one)?.checked_divmod(&divisor)?
    print(max_q == (block - one)?, max_r == remainder)?
    # Normalization shifts a non-word-aligned divisor and leaves low limbs
    # out of the division. Retain those bits in signed floor correction.
    scaled_divisor = (divisor << 68)?
    scaled_a = ((a << 68)? + ((one << 68)? - one)?)?
    (floor, modulus) = (-scaled_a)?.checked_divmod(&scaled_divisor)?
    print(floor == (from_i64(3) - block)?, modulus == one)?
"#;
    let expected = "True True\n".repeat(3);
    assert_eq!(run(body, false), expected);
    assert_eq!(run(body, true), expected);
}

#[test]
fn skewed_multiplication_carries_across_blocks_and_a_short_tail() {
    let body = r#"    one = from_i64(1)
    short_base = (one << 65536)?
    long_base = (one << 197696)?
    short = (short_base - one)?
    long = (long_base - one)?
    # Expand (2^65536 - 1) * (2^197696 - 1) without multiplication.
    expected = ((((one << 263232)? - long_base)? - short_base)? + one)?
    print((short * long)? == expected, (long * (-short)?)? == (-expected)?)?
"#;
    assert_eq!(run(body, false), "True True\n");
    assert_eq!(run(body, true), "True True\n");
}

#[test]
fn barrett_division_handles_rounded_prefixes_and_signed_remainders() {
    let body = r#"    one = from_i64(1)
    base = (one << 393216)?
    # A 6144-limb divisor admits two cyclic correction blocks. The first
    # divisor makes the rounded reciprocal prefix overflow. The second
    # is close to half the limb base and has nonzero omitted limbs.
    divisors = [(base - one)?, (((one << 393215)? + (one << 164999)?)? - one)?]?
    for divisor in &divisors:
        triple = ((divisor << 1)? + *divisor)?
        a = ((((divisor << 393216)?) - triple)? - one)?
        (q, r) = a.checked_divmod(divisor)?
        print(q == (base - from_i64(4))?, r == (divisor - one)?)?
        low = ((one << 68)? - one)?
        shifted_a = ((a << 68)? + low)?
        shifted_divisor = (divisor << 68)?
        (floor, modulus) = (-shifted_a)?.checked_divmod(&shifted_divisor)?
        print(floor == (from_i64(3) - base)?, modulus == one)?
"#;
    let expected = "True True\n".repeat(4);
    assert_eq!(run(body, false), expected);
    assert_eq!(run(body, true), expected);
}

#[test]
fn barrett_cyclic_correction_recovers_limbs_above_the_transform() {
    let body = r#"    one = from_i64(1)
    # 8192 limbs is the transform width in all three cases. The last
    # case also exercises the recursive low product with an odd width.
    for bits in [524288, 524352, 528384]?:
        base = (one << bits)?
        divisors = [(base - one)?, (((base >> 1)? + (one << 170321)?)? + one)?]?
        for divisor in &divisors:
            # Construct d*(B^n-2) from shifts, independently of multiplication.
            exact = (((divisor << bits)?) - (divisor << 1)?)?
            (q, r) = exact.checked_divmod(divisor)?
            print(q == (base - from_i64(2))?, r == from_i64(0))?
            (floor, modulus) = (one - exact)?.checked_divmod(divisor)?
            print(floor == (from_i64(2) - base)?, modulus == one)?
            (previous, near) = (exact - one)?.checked_divmod(divisor)?
            print(previous == (base - from_i64(3))?, near == (divisor - one)?)?
"#;
    let expected = "True True\n".repeat(18);
    assert_eq!(run(body, false), expected);
    assert_eq!(run(body, true), expected);
}

#[test]
fn decimal_render_reuses_a_large_split_power_at_the_exact_cap() {
    let body = r#"    text = "98765432101234567890".repeat(15800)?
    value = parse(text, 10, 316000)?
    negative = (-value)?
    signed = ("-" + text)?
    print(negative.to_string(10, 316000)? == signed)?
    match value.to_string(10, 315999):
        case Err(BigIntError.LimitExceeded):
            print("limit")?
        case _:
            print("wrong result")?
"#;
    assert_eq!(run(body, false), "True\nlimit\n");
    assert_eq!(run(body, true), "True\nlimit\n");
}

#[test]
fn large_products_and_division_cross_algorithm_boundaries() {
    let mut body = String::new();
    let mut expected = String::new();
    for limbs in [31, 32, 33, 64] {
        let n = limbs * 16;
        let input = "f".repeat(n);
        // (16^n - 1)^2 = (16^n - 2) * 16^n + 1.
        let square = format!("{}e{}1", "f".repeat(n - 1), "0".repeat(n - 1));
        body.push_str(&format!("    a{limbs} = parse(\"{input}\", 16)?\n    square{limbs} = (a{limbs} * a{limbs})?\n    print(square{limbs}.to_string(16)? == \"{square}\", (square{limbs} // a{limbs})? == a{limbs}, (square{limbs} % a{limbs})? == from_i64(0))?\n"));
        expected.push_str("True True True\n");
    }
    // Deliberately unbalanced schoolbook multiplication plus a normalized
    // multi-digit divisor whose leading base-2^32 digit is small.
    body.push_str("    a = (from_i64(1) << 4096)?\n    b = parse(\"18446744073709551617\")?\n    product = (a * b)?\n    print((product // b)? == a, (product % b)? == from_i64(0))?\n");
    expected.push_str("True True\n");
    assert_eq!(run(&body, false), expected);
    assert_eq!(run(&body, true), expected);
}

#[test]
fn division_adds_back_when_the_low_digit_invalidates_the_estimate() {
    let body = r#"    base = (from_i64(1) << 32)?
    for text in ["800000000000000000000001", "10000000000000001"]?:
        divisor = parse(text, 16)?
        residue = (divisor - from_i64(1))?
        numerator = ((divisor * base)? + residue)?
        negative = (-numerator)?
        negative_divisor = (-divisor)?
        floor = ((-base)? - from_i64(1))?
        (q, r) = numerator.checked_divmod(&divisor)?
        (nq, nr) = negative.checked_divmod(&divisor)?
        (dq, dr) = numerator.checked_divmod(&negative_divisor)?
        (bq, br) = negative.checked_divmod(&negative_divisor)?
        print(q == base, r == residue, nq == floor, nr == from_i64(1), dq == floor, dr == from_i64(-1), bq == base, br == (-residue)?)?
"#;
    // After subtracting the high quotient digit, the normalized divisor's
    // top two digits match the remainder's. Only the subtract/add-back can
    // distinguish that remainder from one more copy of the divisor.
    let expected = "True True True True True True True True\n".repeat(2);
    assert_eq!(run(body, false), expected);
    assert_eq!(run(body, true), expected);
}

#[test]
fn karatsuba_unequal_halves_and_carry_chains() {
    let mut body = String::new();
    let mut expected = String::new();
    // At exactly 2:1 the shorter operand has an empty high half. Adjacent
    // sizes exercise both sides of the balanced-Karatsuba condition.
    for (index, (a, b)) in [(63, 32), (64, 32), (65, 32), (65, 33), (127, 64)]
        .into_iter()
        .enumerate()
    {
        let x = "f".repeat(a * 16);
        let y = "f".repeat(b * 16);
        // (B^a - 1)(B^b - 1) = B^(a+b) - B^a - B^b + 1.
        let product = format!(
            "{}e{}{}1",
            "f".repeat(b * 16 - 1),
            "f".repeat((a - b) * 16),
            "0".repeat(b * 16 - 1)
        );
        body.push_str(&format!("    x{index} = parse(\"{x}\", 16)?\n    y{index} = parse(\"{y}\", 16)?\n    p{index} = (x{index} * y{index})?\n    print(p{index}.to_string(16, 10000)? == \"{product}\", (p{index} // y{index})? == x{index}, (p{index} % y{index})? == from_i64(0))?\n"));
        expected.push_str("True True True\n");
    }
    assert_eq!(run(&body, false), expected);
    assert_eq!(run(&body, true), expected);
}

#[test]
fn toom3_products_cover_thirds_signs_and_carry_chains() {
    let mut body = String::new();
    let mut expected = String::new();
    for (index, (a, b)) in [
        (104, 104),
        (105, 105),
        (127, 127),
        (128, 128),
        (129, 129),
        (159, 128),
        (160, 128),
        (161, 128),
        (191, 128),
        (192, 128),
        (192, 129),
        (193, 130),
        (255, 255),
        (256, 256),
        (257, 257),
        (511, 511),
    ]
    .into_iter()
    .enumerate()
    {
        let x = "f".repeat(a * 16);
        let y = "f".repeat(b * 16);
        let lo = a.min(b) * 16;
        let hi = a.max(b) * 16;
        let product = format!(
            "{}e{}{}1",
            "f".repeat(lo - 1),
            "f".repeat(hi - lo),
            "0".repeat(lo - 1)
        );
        body.push_str(&format!("    x{index} = parse(\"-{x}\", 16, 100000)?\n    y{index} = parse(\"{y}\", 16, 100000)?\n    print((x{index} * y{index})?.to_string(16, 100000)? == \"-{product}\")?\n"));
        expected.push_str("True\n");
    }
    // These full-size inputs force opposite signs at evaluation point -1.
    // Multiplication by (2^shift+1) has an independent shift/add oracle.
    for (index, n) in [128usize, 129, 192, 256, 511].into_iter().enumerate() {
        let k = n.div_ceil(3);
        let shift = 64 * n - 1;
        let third_shift = 64 * k;
        body.push_str(&format!("    high{index} = (from_i64(1) << {shift})?\n    middle{index} = (((from_i64(1) << {third_shift})? - from_i64(1))? << {third_shift})?\n    a{index} = ((high{index} + middle{index})? + from_i64(1))?\n    b{index} = (high{index} + from_i64(1))?\n    want{index} = ((a{index} << {shift})? + a{index})?\n    print((a{index} * b{index})? == want{index}, ((-a{index})? * b{index})? == (-want{index})?)?\n"));
        expected.push_str("True True\n");
    }
    assert_eq!(run(&body, false), expected);
    assert_eq!(run(&body, true), expected);
}

#[test]
fn transform_products_cover_padding_carries_signs_and_unequal_operands() {
    let mut body = String::new();
    let mut expected = String::new();
    // Exercise the size threshold and truncation just above a power of two.
    // All-one digits maximize convolution coefficients and carry chains.
    for (index, (a, b)) in [
        (1023, 1023),
        (1024, 1024),
        (1024, 1025),
        (1025, 1025),
        (1535, 1535),
        (1536, 1536),
        (1537, 1537),
        (1791, 1791),
        (1792, 1792),
        (1793, 1793),
        (2048, 1024),
        (1024, 2048),
        (1025, 3092),
    ]
    .into_iter()
    .enumerate()
    {
        let x = "f".repeat(a * 16);
        let y = "f".repeat(b * 16);
        let lo = a.min(b) * 16;
        let hi = a.max(b) * 16;
        let product = format!(
            "{}e{}{}1",
            "f".repeat(lo - 1),
            "f".repeat(hi - lo),
            "0".repeat(lo - 1)
        );
        body.push_str(&format!("    x{index} = parse(\"{x}\", 16, 100000)?\n    y{index} = parse(\"-{y}\", 16, 100000)?\n    p{index} = (x{index} * y{index})?\n    print(p{index}.to_string(16, 100000)? == \"-{product}\")?\n"));
        expected.push_str("True\n");
    }
    // Sparse operands exercise zero coefficients and a carry into the top limb.
    body.push_str("    a = (from_i64(1) << 65535)?\n    p = (a * a)?\n    print(p == (from_i64(1) << 131070)?, a.bit_length() == 65536)?\n");
    expected.push_str("True True\n");
    assert_eq!(run(&body, false), expected);
    assert_eq!(run(&body, true), expected);
}

#[cfg(feature = "runtime-checks")]
#[test]
fn transform_allocation_failures_preserve_operands() {
    let output = run(
        r#"    a = (from_i64(1) << 65535)?
    original = copy(a)?
    mut failures = 0
    for budget in range(80):
        marker = f"__test_fail_allocations_after_{budget}__"?
        print(marker)?
        product = a * a
        print("__test_restore_allocations__")?
        if product.is_err():
            unchecked:
                failures += 1
        if a != original:
            return Err(BigIntError.OutOfRange)
    print(failures > 0, a == original)?
"#,
        true,
    );
    assert!(output.ends_with("True True\n"), "{output}");
}

#[cfg(feature = "runtime-checks")]
#[test]
fn toom3_allocation_failures_preserve_operands() {
    let output = run(
        r#"    a = (from_i64(1) << 8191)?
    original = copy(a)?
    mut failures = 0
    for budget in range(80):
        marker = f"__test_fail_allocations_after_{budget}__"?
        print(marker)?
        product = a * a
        print("__test_restore_allocations__")?
        if product.is_err():
            unchecked:
                failures += 1
        if a != original:
            return Err(BigIntError.OutOfRange)
    print(failures > 0, a == original)?
"#,
        true,
    );
    assert!(output.ends_with("True True\n"), "{output}");
}

#[test]
fn limb_destructuring_keeps_scalar_values() {
    let dir = tempfile::tempdir().unwrap();
    let source = r#"def product(a: u64, b: u64) -> u64:
    (low, high) = a.widening_mul(b)
    low ^ high
def sum(a: u64, b: u64, carry: bool) -> u64:
    (word, overflow) = a.carrying_add(b, carry)
    word ^ (1u64 if overflow else 0u64)
def main() -> Result[(), IoError]:
    print(product(u64.MAX, u64.MAX), sum(u64.MAX, 1u64, True))?
    Ok(())
"#;
    assert_eq!(run_program(source, true), "18446744073709551615 0\n");
    let path = dir.path().join("main.plenty");
    std::fs::write(&path, source).unwrap();
    let dump = dir.path().join("dump");
    let output = Command::new(env!("CARGO_BIN_EXE_plenty"))
        .env("PLENTY_CACHE", "off")
        .args(["--dump", "clif-opt", "--dump-filter", "product"])
        .arg(&path)
        .arg("-o")
        .arg(&dump)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let clif = std::fs::read_to_string(dump.join("clif-opt.txt")).unwrap();
    assert!(clif.contains("umulhi"), "{clif}");
    assert!(!clif.contains("explicit_slot"), "{clif}");
}

#[cfg(feature = "runtime-checks")]
#[test]
fn allocation_failures_propagate_and_inline_arithmetic_does_not_allocate() {
    let output = run(
        r#"    large = parse("18446744073709551616")?
    mut destination = parse("18446744073709551616")?
    print("__test_fail_allocations_after_0__")?
    print("__test_begin_no_allocations__")?
    a = from_i64(41)
    b = from_i64(-7)
    sum = (a + b)?
    difference = (a - b)?
    product = (a * b)?
    quotient = (a // b)?
    remainder = (a % b)?
    shifted = (a << 2)?
    right = (b >> 5)?
    bits = (a ^ b)?
    inline_parsing = parse("-9223372036854775808")
    zero_parsing = parse("+0000000000000000000000000000", 16)
    print("__test_end_no_allocations__")?
    addition = large + from_i64(1)
    multiplication = large * large
    division = large.checked_divmod(&from_i64(3))
    parsing = parse("18446744073709551616")
    packed = parse("10000000000000000", 16)
    rendering = large.to_string()
    assignment = destination.add_assign(&large)
    print("__test_restore_allocations__")?
    print(sum, difference, product, quotient, remainder, shifted, right, bits)?
    print(inline_parsing? == from_i64(i64.MIN), zero_parsing? == from_i64(0))?
    print(addition.is_err(), multiplication.is_err(), division.is_err(), parsing.is_err(), packed.is_err(), rendering.is_err(), assignment.is_err(), destination == large)?
"#,
        false,
    );
    assert!(
        output.ends_with(
            "34 48 -287 -6 -1 164 -1 -48\nTrue True\nTrue True True True True True True True\n"
        ),
        "{output}"
    );
}

#[cfg(feature = "runtime-checks")]
#[test]
fn partial_allocation_failures_leave_operands_and_assignment_intact() {
    let output = run(
        r#"    seed = parse("12345678901234567890123456789012345678901234567890")?
    for budget in range(20):
        mut destination = copy(seed)?
        marker = f"__test_fail_allocations_after_{budget}__"?
        print(marker)?
        result = destination.add_assign(&seed)
        product = seed * seed
        parsed = parse("12345678901234567890123456789012345678901234567890")
        rendered = seed.to_string()
        print("__test_restore_allocations__")?
        if result.is_err():
            if destination != seed:
                return Err(BigIntError.OutOfRange)
        drop(product)
        drop(parsed)
        drop(rendered)
    print("preserved")?
"#,
        false,
    );
    assert!(output.ends_with("preserved\n"), "{output}");
}

#[test]
fn formatting_hooks_respect_the_active_sum_payload() {
    let source = r#"from std.fmt import Formatter, FormatError
class Pair:
    a: i64
    b: i64
enum Value:
    Small(i64)
    Large(Pair, list[u64])
    def debug[W](self, out: &mut Formatter[W]) -> Result[(), FormatError]:
        match self:
            case Value.Small(n):
                out.text("small:")?
                out.text(str.repr(*n).map_error(FormatError.Allocation)?)
            case Value.Large(pair, words):
                out.text("large:")?
                out.text(str.repr(pair.a).map_error(FormatError.Allocation)?)?
                out.text(str.repr(words[0]).map_error(FormatError.Allocation)?)
class Number:
    value: Value
    def display[W](self, out: &mut Formatter[W]) -> Result[(), FormatError]:
        self.value.debug(out)
def main() -> Result[(), AllocError | IoError | FormatError]:
    small = Number(Value.Small(7))
    large = Number(Value.Large(Pair(3, 4), [9u64]?))
    print(small, large)?
    values = [Value.Small(8), Value.Large(Pair(5, 6), [2u64]?)]?
    print(values)?
    print(small, large)?
    Ok(())
"#;
    for optimize in [false, true] {
        assert_eq!(
            run_program(source, optimize),
            "small:7 large:39\n[small:8, large:52]\nsmall:7 large:39\n"
        );
    }
}

#[test]
fn zero_divisors_and_negative_shifts_trap_at_the_operation() {
    crate::test_support::assert_trap_cases(
        r#"from std.bigint import from_i64
def main() -> Result[(), AllocError | IoError | ParseError]:
    argv = args()?
    index = i64.parse(argv[1])?
    if index == 0:
        drop(from_i64(1) // from_i64(0))
    elif index == 1:
        drop(from_i64(1) % from_i64(0))
    elif index == 2:
        drop(from_i64(1) << -1)
    else:
        drop(from_i64(1) >> -1)
    Ok(())
"#,
        &[
            "BigInt.__floordiv__: divisor is zero".into(),
            "BigInt.__mod__: divisor is zero".into(),
            "BigInt.__lshift__: count is negative".into(),
            "BigInt.__rshift__: count is negative".into(),
        ],
    );
}
