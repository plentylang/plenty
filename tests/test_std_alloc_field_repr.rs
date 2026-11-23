use super::test_std_alloc::assert_stdout;
use super::test_std_alloc_set_algebra::POLICY;

#[test]
fn aggregate_field_scratch_refusal_precedes_field_emission() {
    let body = r#"
import std.fmt as fmt

def main() -> Result[(), FormatError | AllocError | IoError]:
    rows = [("é", 12)]?
    for blocks in range(4):
        scratch = budget("field", blocks, AllocError.CapacityOverflow)
        mut out = fmt.string_formatter(80, 1)?
        match out.format_in(&scratch, f"pre:{rows}"):
            case Err(error):
                print(error, out.completed(), fmt.finish(&out)?)?
            case Ok(_):
                print(fmt.finish(&out)?)?
    empty: list[i64] = []?
    denied = budget("inline", 0)
    mut out = fmt.string_formatter(80, 1)?
    match out.format_in(&denied, f"{empty}"):
        case Err(error):
            print(error, out.completed())?
        case Ok(_):
            print("unexpected")?
    Ok(())
"#;
    assert_stdout(
        &format!("{POLICY}{body}"),
        "refused field\nstd.fmt.FormatError.Allocation(AllocError.CapacityOverflow) 4 pre:\nfield 0 0\nrefused field\nstd.fmt.FormatError.Allocation(AllocError.CapacityOverflow) 4 pre:\nfield 0 1\nrefused field\nstd.fmt.FormatError.Allocation(AllocError.CapacityOverflow) 4 pre:\nfield 0 2\npre:[(\"é\", 12)]\nfield 0 3\nrefused inline\nstd.fmt.FormatError.Allocation(AllocError.OutOfMemory) 0\ninline 0 0\n",
    );
}

#[test]
fn structural_fields_keep_repr_layout_ascii_and_partial_limits() {
    assert_stdout(
        r#"import std.alloc as alloc
import std.fmt as fmt

def main() -> Result[(), FormatError | AllocError | IoError]:
    scratch = alloc.system()
    rows = [("é\n", -9), ("🙂", 2)]?
    mut out = fmt.string_formatter(150, 0)?
    out.format_in(&scratch, f"{rows!r}|{&rows[0:1]!s:🙂>17.9}|{rows[1]!a}")?
    print(fmt.finish(&out)?)?
    mut escaped = fmt.string_formatter(32, 0)?
    escaped.format_in(&scratch, f"{rows[1]!a:>15.9}")?
    print(fmt.finish(&escaped)?)?
    mut limited = fmt.string_formatter(5, 0)?
    match limited.format_in(&scratch, f"x{rows}"):
        case Err(error):
            print(error, fmt.finish(&limited)?, limited.completed())?
        case Ok(_):
            print("unexpected")?
    match limited.format_in(&scratch, f"{rows:>30.3}"):
        case Err(error):
            print(error, limited.completed())?
        case Ok(_):
            print("unexpected")?
    Ok(())
"#,
        "[(\"é\\n\", -9), (\"🙂\", 2)]|🙂🙂🙂🙂🙂🙂🙂🙂[(\"é\\n\", |(\"\\u{1f642}\", 2)\n      (\"\\u{1f64\nstd.fmt.FormatError.LimitExceeded x 1\nstd.fmt.FormatError.LimitExceeded 5\n",
    );
}

#[cfg(feature = "runtime-checks")]
#[test]
fn structural_field_needs_no_unselected_allocations() {
    assert_stdout(
        r#"import std.alloc as alloc
import std.fmt as fmt

pub class Pool:
    blocks: list[alloc.ByteBlock]
    pub def allocate(self: &mut Pool, request: &alloc.Layout) -> Result[alloc.ByteBlock, AllocError]:
        self.blocks.pop().ok_or(AllocError.OutOfMemory)
    pub def release(self: &mut Pool, block: alloc.ByteBlock) -> ():
        drop(block)

def main() -> Result[(), FormatError | AllocError | IoError]:
    root = alloc.system()
    layout = alloc.layout(1024, 64).unwrap()
    blocks = [root.block(&layout)?, root.block(&layout)?, root.block(&layout)?, root.block(&layout)?]?
    field = alloc.allocator(&root, Pool(blocks)).unwrap()
    rows = [["é\n", "🙂"]?]?
    mut out = fmt.string_formatter(32, 0)?
    out.writer.data.reserve(32)?
    print("__test_fail_allocations_after_0__")?
    print("__test_begin_no_allocations__")?
    rendered = out.format_in(&field, f"{rows}")
    print("__test_end_no_allocations__")?
    print("__test_restore_allocations__")?
    rendered?
    print(fmt.finish(&out)?)?
    Ok(())
"#,
        "__test_fail_allocations_after_0__\n__test_begin_no_allocations__\n__test_end_no_allocations__\n__test_restore_allocations__\n[[\"é\\n\", \"🙂\"]]\n",
    );
}

#[test]
fn structural_spelling_keeps_partial_writer_error() {
    assert_stdout(
        r#"import std.alloc as alloc
import std.bytes as binary
import std.fmt as fmt
from std.io import StreamError

pub class Sink:
    left: i64
    pub def flush(self: &mut Sink) -> Result[(), StreamError]:
        Ok(())
    pub def write(self: &mut Sink, source: &[u8]) -> Result[i64, StreamError]:
        if self.left == 0:
            return Ok(0)
        unchecked:
            self.left -= 1
        Ok(1)

def main() -> Result[(), FormatError | AllocError | IoError]:
    work = alloc.system()
    mut scratch = binary.with_capacity(32)?
    scratch.resize(32, 0u8)?
    mut out = fmt.formatter(Sink(4), scratch, 80, 0)?
    rows = ["🙂"]?
    match out.format_in(&work, f"{rows}"):
        case Err(error):
            print(error, out.completed())?
        case Ok(_):
            print("unexpected")?
    Ok(())
"#,
        "std.fmt.FormatError.Output(std.io.StreamError.WriteZero) 4\n",
    );
}

#[test]
fn selected_aggregate_field_preserves_provider_wait_checks() {
    super::test_std_alloc_slice_clist::check_provider_join(
        r#"import std.fmt as fmt

def work(field: &alloc.Allocator) -> ():
    mut output = fmt.string_formatter(32, 0).unwrap()
    pair = (1, 2)
    drop(output.format_in(field, f"{pair}"))"#,
        "with spawn(work, &out).drop_job().unwrap() as task:\n        with guard.lock():\n            task.join()",
        "\"ordinary\"",
        true,
        Some("worker can lock a `Mutex[i64]`"),
    );
}

#[test]
fn selected_structural_walk_never_swallows_nested_debug_hooks() {
    let error = crate::test_support::check_error(
        r#"import std.alloc as alloc
import std.fmt as fmt

class Label:
    def debug[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        out.text("hook")

def main() -> Result[(), FormatError | AllocError]:
    field = alloc.system()
    mut out = fmt.string_formatter(32, 8)?
    rows = [(Label(), 1)]?
    out.format_in(&field, f"{rows}")
"#,
    );
    assert!(
        error.contains("does not yet support selected scratch"),
        "{error}"
    );
}
