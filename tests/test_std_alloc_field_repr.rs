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

class Mark:
    def debug[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        out.text("🙂")

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
    text = ["🙂"]?
    match out.format_in(&work, f"{text}"):
        case Err(error):
            print(error, out.completed())?
        case Ok(_):
            print("unexpected")?
    mut scratch2 = binary.with_capacity(32)?
    scratch2.resize(32, 0u8)?
    mut padded = fmt.formatter(Sink(4), scratch2, 80, 0)?
    rows = [Mark()]?
    match padded.format_in(&work, f"{rows:>6}"):
        case Err(error):
            print(error, padded.completed())?
        case Ok(_):
            print("unexpected")?
    Ok(())
"#,
        "std.fmt.FormatError.Output(std.io.StreamError.WriteZero) 4\nstd.fmt.FormatError.Output(std.io.StreamError.WriteZero) 4\n",
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
fn nested_debug_stages_once_in_walk_order_even_when_truncated() {
    assert_stdout(
        r#"import std.alloc as alloc
import std.fmt as fmt

class Label:
    id: str
    def display[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        out.text("display")
    def debug[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        print(self.id).unwrap()
        # The adapter has entered once; its private depth is 32, not the
        # destination's zero. Check both the last allowed and first denied enter.
        for _ in range(31):
            out.enter()?
        match out.enter():
            case Err(FormatError.RecursionLimit):
                pass
            case _:
                return Err(FormatError.InvalidSpec)
        for _ in range(31):
            out.leave()
        out.text(&self.id)

def width() -> i64:
    print("width").unwrap()
    5

def main() -> Result[(), FormatError | AllocError | IoError]:
    field = alloc.system()
    rows = [Label("é"), Label("B")]?
    mut out = fmt.string_formatter(96, 0)?
    out.format_in(&field, f"{rows:·>{width()}.3}/{&rows[0:1]!a:>10}/{rows!s}")?
    print(fmt.finish(&out)?)?
    print(rows)?
    Ok(())
"#,
        "width\né\nB\né\né\nB\n··[é,/  [\\u{e9}]/[é, B]\né\nB\n[é, B]\n",
    );
}

#[test]
fn nested_debug_refusal_and_hook_error_discard_only_current_field() {
    let body = r#"
import std.fmt as fmt

class Label:
    id: str
    fails: bool
    def debug[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        print(self.id).unwrap()
        out.text(&self.id)?
        if self.fails:
            return Err(FormatError.InvalidSpec)
        Ok(())

def main() -> Result[(), FormatError | AllocError | IoError]:
    rows = [Label("a", False), Label("b", True), Label("c", False)]?
    for count in [6, 20]?:
        storage = budget("field", count, AllocError.CapacityOverflow)
        mut out = fmt.string_formatter(64, 0)?
        match out.format_in(&storage, f"n={7}|{rows:>18}"):
            case Err(error):
                print(error, out.completed(), fmt.finish(&out)?)?
            case Ok(_):
                print("unexpected")?
    Ok(())
"#;
    assert_stdout(
        &format!("{POLICY}{body}"),
        "a\nrefused field\nstd.fmt.FormatError.Allocation(AllocError.CapacityOverflow) 4 n=7|\nfield 0 6\na\nb\nstd.fmt.FormatError.InvalidSpec 4 n=7|\nfield 11 9\n",
    );
}

#[cfg(feature = "runtime-checks")]
#[test]
fn nested_debug_selects_large_receiver_and_ascii_staging_not_callback_allocations() {
    assert_stdout(
        r#"import std.alloc as alloc
import std.fmt as fmt

pub class Pool:
    blocks: list[alloc.ByteBlock]
    pub def allocate(self: &mut Pool, request: &alloc.Layout) -> Result[alloc.ByteBlock, AllocError]:
        self.blocks.pop().ok_or(AllocError.OutOfMemory)
    pub def release(self: &mut Pool, storage: alloc.ByteBlock) -> ():
        self.blocks.append(storage).unwrap()

enum Entry:
    Wide(array[i64, 96], str)
    Private
    def debug[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        match self:
            case Entry.Wide(values, label):
                out.integer(values[90], 0)?
                out.text(label)
            case Entry.Private:
                # A callback's own allocation still fails independently.
                text = ("private" + " spelling").map_error(FormatError.Allocation)?
                out.text(&text)

def main() -> Result[(), FormatError | AllocError | IoError]:
    root = alloc.system()
    layout = alloc.layout(4096, 64).unwrap()
    mut blocks: list[alloc.ByteBlock] = []?
    for _ in range(12):
        blocks.append(root.block(&layout)?)?
    field = alloc.allocator(&root, Pool(blocks)).unwrap()
    rows = [Entry.Wide(array[i64, 96].filled(9), "éabcdefg")]?
    bad = [Entry.Private]?
    mut out = fmt.string_formatter(128, 0)?
    out.writer.data.reserve(128)?
    print("__test_fail_allocations_after_0__")?
    print("__test_begin_no_allocations__")?
    rendered = out.format_in(&field, f"{rows!a:>20}")
    print("__test_end_no_allocations__")?
    failed = out.format_in(&field, f"|{bad}")
    print("__test_restore_allocations__")?
    rendered?
    match failed:
        case Err(error):
            print(error, fmt.finish(&out)?)?
        case Ok(_):
            print("unexpected")?
    Ok(())
"#,
        "__test_fail_allocations_after_0__\n__test_begin_no_allocations__\n__test_end_no_allocations__\n__test_restore_allocations__\nstd.fmt.FormatError.Allocation(AllocError.OutOfMemory)     [9\\u{e9}abcdefg]|\n",
    );
}

#[test]
fn nested_debug_worker_checks_include_callback_body() {
    let error = crate::test_support::check_error(
        r#"import std.alloc as alloc
import std.fmt as fmt
import std.env

class Label:
    def debug[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        path = std.env.current_dir().unwrap()
        std.env.set_current_dir(&path).unwrap()
        out.text("x")

def work() -> ():
    storage = alloc.system()
    mut out = fmt.string_formatter(32, 0).unwrap()
    pair = (Label(), 1)
    out.format_in(&storage, f"{pair}").unwrap()

def main() -> ():
    with spawn(work).drop_job().unwrap() as task:
        task.join()
"#,
    );
    assert!(error.contains("changing the working directory"), "{error}");
}

#[test]
fn nested_debug_discovery_matches_ordinary_fixed_array_fields() {
    assert_stdout(
        r#"import std.alloc as alloc
import std.fmt as fmt

@copy
class Item:
    n: i64
    def debug[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        print("hook").unwrap()
        out.text("item")

def main() -> Result[(), FormatError | AllocError | IoError]:
    scratch = alloc.system()
    rows = array[Item, 2].filled(Item(1))
    mut out = fmt.string_formatter(100, 0)?
    out.format(f"{rows}|")?
    out.format_in(&scratch, f"{rows}")?
    print(fmt.finish(&out)?)?
    Ok(())
"#,
        "array[Item(n=1), Item(n=1)]|array[Item(n=1), Item(n=1)]\n",
    );
}
