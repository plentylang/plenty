use super::test_std_alloc::assert_stdout;
use super::test_std_alloc_set_algebra::POLICY;

#[test]
fn padded_hook_scratch_refuses_and_unwinds_each_stage_without_replaying() {
    assert_stdout(
        &format!(
            r#"{POLICY}
import std.fmt as fmt

class Label:
    def display[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        print("display").unwrap()
        out.text("abcdefgh🙂")

def main() -> Result[(), FormatError | IoError]:
    label = Label()
    for count in range(6):
        storage = budget("field", count, AllocError.CapacityOverflow)
        mut out = fmt.string_formatter(32, 4)?
        match out.format_in(&storage, f"|{{label:🙂^5.3}}|"):
            case Err(error):
                print(error, fmt.finish(&out)?, out.completed())?
            case Ok(_):
                print(fmt.finish(&out)?, out.completed())?
    Ok(())
"#
        ),
        concat!(
            "refused field\nstd.fmt.FormatError.Allocation(AllocError.CapacityOverflow) | 1\nfield 0 0\n",
            "refused field\nstd.fmt.FormatError.Allocation(AllocError.CapacityOverflow) | 1\nfield 0 1\n",
            "refused field\nstd.fmt.FormatError.Allocation(AllocError.CapacityOverflow) | 1\nfield 0 2\n",
            "display\nrefused field\nstd.fmt.FormatError.Allocation(AllocError.CapacityOverflow) | 1\nfield 0 3\n",
            "display\nrefused field\nstd.fmt.FormatError.Allocation(AllocError.CapacityOverflow) | 1\nfield 0 4\n",
            "display\n|🙂abc🙂| 13\nfield 0 5\n",
        ),
    );
}

#[test]
fn padded_hook_scratch_preserves_hook_limit_and_writer_errors() {
    assert_stdout(
        &format!(
            r#"{POLICY}
import std.fmt as fmt
import std.bytes as binary
from std.io import StreamError

class Label:
    fails: bool
    def debug[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        print("debug").unwrap()
        out.text("é🙂abcdef")?
        if self.fails:
            return Err(FormatError.InvalidSpec)
        Ok(())

pub class Sink:
    left: i64
    pub def write(self: &mut Sink, source: &[u8]) -> Result[i64, StreamError]:
        if self.left == 0:
            return Ok(0)
        unchecked:
            self.left -= 1
        Ok(1)
    pub def flush(self: &mut Sink) -> Result[(), StreamError]:
        Ok(())

def main() -> Result[(), AllocError | FormatError | IoError]:
    storage = budget("all", 30)
    good = Label(False)
    bad = Label(True)
    mut limited = fmt.string_formatter(6, 0)?
    match limited.format_in(&storage, f"|{{bad:>10}}"):
        case Err(error):
            print(error, limited.completed())?
        case Ok(_):
            print("unexpected")?
    match limited.format_in(&storage, f"{{good!r:>10}}"):
        case Err(error):
            print(error, limited.completed(), fmt.finish(&limited)?)?
        case Ok(_):
            print("unexpected")?
    mut bytes = binary.with_capacity(32)?
    bytes.resize(32, 0u8)?
    mut stream = fmt.formatter(Sink(3), bytes, 64, 4)?
    match stream.format_in(&storage, f"{{good:>10.2}}"):
        case Err(error):
            print(error, stream.completed(), stream.has_failed())?
        case Ok(_):
            print("unexpected")?
    Ok(())
"#
        ),
        "debug\nstd.fmt.FormatError.InvalidSpec 1\ndebug\nstd.fmt.FormatError.LimitExceeded 3 |  \ndebug\nstd.fmt.FormatError.Output(std.io.StreamError.WriteZero) 3 False\nall 16 14\n",
    );
}

#[cfg(feature = "runtime-checks")]
#[test]
fn padded_hook_scratch_uses_no_ordinary_text_even_for_ascii_conversion() {
    assert_stdout(
        r#"import std.alloc as alloc
import std.fmt as fmt

pub class Pool:
    blocks: list[alloc.ByteBlock]
    pub def allocate(self: &mut Pool, request: &alloc.Layout) -> Result[alloc.ByteBlock, AllocError]:
        self.blocks.pop().ok_or(AllocError.OutOfMemory)
    pub def release(self: &mut Pool, storage: alloc.ByteBlock) -> ():
        # Return the block to the already reserved free list.
        self.blocks.append(storage).unwrap()

class Label:
    def display[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        print("display").unwrap()
        out.text("🙂abcdefgh")
    def debug[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        print("debug").unwrap()
        out.text("é🙂abcdef")

def main() -> Result[(), AllocError | IoError | FormatError]:
    root = alloc.system()
    layout = alloc.layout(1024, 64).unwrap()
    mut blocks: list[alloc.ByteBlock] = []?
    for _ in range(12):
        blocks.append(root.block(&layout)?)?
    storage = alloc.allocator(&root, Pool(blocks)).unwrap()
    mut out = fmt.string_formatter(128, 0)?
    out.writer.data.reserve(128)?
    label = Label()
    width = 4
    print("__test_fail_allocations_after_0__")?
    print("__test_begin_no_allocations__")?
    result = out.format_in(&storage, f"{label!s:·^{width}.2}/{label!r:.3}/{label!a:>10.7}/{label!a}")
    print("__test_end_no_allocations__")?
    print("__test_restore_allocations__")?
    result?
    print(fmt.finish(&out)?)?
    Ok(())
"#,
        "__test_fail_allocations_after_0__\n__test_begin_no_allocations__\ndisplay\ndebug\ndebug\ndebug\n__test_end_no_allocations__\n__test_restore_allocations__\n·🙂a·/é🙂a/   \\u{e9}\\/\\u{e9}\\u{1f642}abcdef\n",
    );
}

#[test]
fn padded_hook_scratch_does_not_import_allocator_operations_into_ordinary_actor_code() {
    assert_stdout(
        r#"import std.actor as actors
import std.fmt as fmt
class Label:
    def display[W](self, out: &mut fmt.Formatter[W]) -> Result[(), FormatError]:
        out.text("ok")
def main() -> Result[(), FormatError | IoError]:
    mut out = fmt.string_formatter(32, 4)?
    out.format(f"{Label():>4}")?
    print(actors.heaps_live(), fmt.finish(&out)?)?
    Ok(())
"#,
        "0   ok\n",
    );
}
