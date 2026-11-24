//! Success variants from inlined returns and match arms jump past the `?`
//! tag test. Failure variants and values with unknown tags keep their paths.
use std::path::Path;
use std::process::Command;

mod support;

fn plenty(dir: &Path, args: &[&str]) -> String {
    let output = Command::new(support::plenty_bin())
        .current_dir(dir)
        .env("PLENTY_CACHE", "off")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

const RETURNS: &str = r#"
class Counter:
    n: i64

    def step(self: &mut Counter) -> Result[i64, OverflowError[i64]]:
        self.n = self.n * 3 + 1
        Ok(self.n)


def climb(counter: &mut Counter, steps: i64) -> Result[i64, OverflowError[i64]]:
    mut last = 0
    for _ in range(steps):
        last = counter.step()?
    Ok(last)


def even(x: i64) -> Option[i64]:
    if x % 2 == 0:
        return Some(x)
    Nothing


def large(x: i64) -> Option[bool]:
    Some(even(x)? > 4)


def positive(x: f64) -> Option[f64]:
    if x > 0.0:
        return Some(x)
    Nothing


def halved(x: f64) -> Option[f64]:
    Some(positive(x)? / 2.0)


def small(x: i64) -> Option[bool]:
    if x < 0:
        return Nothing
    Some(x < 10)


def negated(x: i64) -> Option[bool]:
    Some(not small(x)?)


def either(x: i64, fallback: Result[i64, OverflowError[i64]]) -> Result[i64, OverflowError[i64]]:
    if x > 0:
        return Ok(x)
    fallback


def after(x: i64, fallback: Result[i64, OverflowError[i64]]) -> Result[i64, OverflowError[i64]]:
    Ok(either(x, fallback)? + 1)


def narrow(x: u8) -> Result[u8, OverflowError[u8]]:
    Ok(x + 200u8)


def widened(x: u8) -> Result[i64, OverflowError[u8]]:
    Ok(i64(narrow(x)?))


def show(result: Result[i64, OverflowError[i64]]) -> Result[(), IoError]:
    match result:
        case Ok(value):
            print("ok", value)?
        case Err(_):
            print("overflow")?
    Ok(())


def main() -> Result[(), AllocError | IoError]:
    mut counter = Counter(1)
    show(climb(&mut counter, 5))?
    show(climb(&mut counter, 60))?
    print(large(4), large(6), large(5))?
    print(halved(3.0), halved(-3.0))?
    print(negated(3), negated(30), negated(-1))?
    show(after(4, Ok(10)))?
    show(after(0, Ok(10)))?
    show(after(0, climb(&mut counter, 60)))?
    match widened(5u8):
        case Ok(value):
            print("ok", value)?
        case Err(_):
            print("overflow")?
    match widened(100u8):
        case Ok(value):
            print("ok", value)?
        case Err(_):
            print("overflow")?
    Ok(())
"#;

const EXPECTED: &str = "\
ok 364
overflow
Option[bool].Some(False) Option[bool].Some(True) Option[bool].Nothing
Option[f64].Some(1.5) Option[f64].Nothing
Option[bool].Some(False) Option[bool].Some(True) Option[bool].Nothing
ok 5
ok 11
overflow
ok 205
overflow
";

/// Success and failure returns of inlined callees reach the right side of
/// `?`: `Result` and `Option`, one-word payloads of several types, a callee
/// that also returns a value it did not build, and a failing `checked` body.
#[test]
fn known_and_unknown_returns_reach_the_right_side_of_try() {
    for mode in ["--fast-compile", "--release"] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.plenty"), RETURNS).unwrap();
        assert_eq!(
            plenty(dir.path(), &[mode, "main.plenty"]),
            EXPECTED,
            "{mode}"
        );
    }
}

/// `counter.step()?` in `climb`: the method's `Ok` jumps to the code after
/// `?`, and its overflow exits go to the error return, so no tag test is left.
/// `after` keeps the test for the `fallback` it passes through.
#[test]
fn a_known_ok_return_skips_the_tag_test() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.plenty"), RETURNS).unwrap();
    plenty(
        dir.path(),
        &[
            "--release",
            "--dump",
            "clif-opt",
            "main.plenty",
            "-o",
            "out",
        ],
    );
    let clif = std::fs::read_to_string(dir.path().join("out/clif-opt.txt")).unwrap();
    let climb = support::asm_body(&clif, "climb");
    assert!(climb.contains("smul_overflow"), "{climb}");
    assert!(!climb.contains("icmp eq"), "{climb}");
    let after = support::asm_body(&clif, "after");
    assert!(after.contains("icmp eq"), "{after}");
}

const MAPPED_CONDITION: &str = r#"
def mapped_condition(prior: Option[i64], i: i64, k: i64) -> Result[i64, OverflowError[i64]]:
    nearby = prior.map_or(
        Result[bool, OverflowError[i64]].Ok(False),
        def [i, k](j: i64) -> Result[bool, OverflowError[i64]]: Ok(i - j <= k),
    )?
    if nearby:
        return Ok(1)
    Ok(2)

def matched(prior: Option[i64], i: i64, k: i64) -> Result[bool, OverflowError[i64]]:
    match prior:
        case Some(j):
            Ok(i - j <= k)
        case Nothing:
            Ok(False)

def matched_condition(prior: Option[i64], i: i64, k: i64) -> Result[i64, OverflowError[i64]]:
    if matched(prior, i, k)?:
        return Ok(1)
    Ok(2)

def with_fallback(prior: Option[i64], fallback: Result[bool, OverflowError[i64]], i: i64) -> Result[bool, OverflowError[i64]]:
    nearby = prior.map_or(fallback, def [i](j: i64) -> Result[bool, OverflowError[i64]]: Ok(i - j <= 2))?
    Ok(not nearby)

def show(result: Result[i64, OverflowError[i64]]) -> Result[(), IoError]:
    match result:
        case Ok(value):
            print(value)?
        case Err(_):
            print("overflow")?
    Ok(())

def main() -> Result[(), IoError]:
    absent: Option[i64] = Nothing
    show(mapped_condition(absent, i64.MIN, 2))?
    show(mapped_condition(Some(10), 12, 2))?
    show(mapped_condition(Some(10), 13, 2))?
    show(mapped_condition(Some(1), i64.MIN, 2))?
    show(matched_condition(absent, i64.MIN, 2))?
    show(matched_condition(Some(10), 12, 2))?
    show(matched_condition(Some(10), 13, 2))?
    show(matched_condition(Some(1), i64.MIN, 2))?
    print(with_fallback(absent, Ok(False), 12).unwrap())?
    print(with_fallback(Some(10), Ok(False), 12).unwrap())?
    err = with_fallback(Some(1), Ok(False), i64.MIN)
    print(with_fallback(absent, err, 12).is_err())?
    Ok(())
"#;

#[test]
fn mapped_checked_condition_propagates_overflow_and_branches_on_success() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.plenty"), MAPPED_CONDITION).unwrap();
    for mode in ["--fast-compile", "--release"] {
        assert_eq!(
            plenty(dir.path(), &[mode, "main.plenty"]),
            "2\n1\n2\noverflow\n2\n1\n2\noverflow\nTrue\nFalse\nTrue\n",
            "{mode}",
        );
    }

    plenty(
        dir.path(),
        &["--dump", "clif-opt", "main.plenty", "-o", "out"],
    );
    let clif = std::fs::read_to_string(dir.path().join("out/clif-opt.txt")).unwrap();
    let mapped = support::asm_body(&clif, "mapped_condition");
    assert!(mapped.contains("ssub_overflow"), "{mapped}");
    assert!(!mapped.contains("icmp eq"), "{mapped}");
    assert!(!mapped.contains("i64x2"), "{mapped}");
}
