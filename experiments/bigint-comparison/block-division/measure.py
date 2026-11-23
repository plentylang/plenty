"""Compare optimized benchmark executables using file-backed operands."""
import argparse
import csv
import os
import random
import statistics
import subprocess
import tempfile
from pathlib import Path


def workloads(bits, rng, shape_sweep):
    a, b, q = [(1 << (bits - 1)) | rng.getrandbits(bits - 1) for _ in range(3)]
    yield "balanced", "div", b * q + 42, b, 10
    yield "short", "div", b * (q >> (bits - bits // 4)) + 42, b, 10
    for case, radix in [("decimal", 10), ("ternary", 3), ("base36", 36)]:
        yield case, "render", a, b, radix
    yield "dense", "mul", a, b, 10
    for case, radix in [
        ("decimal", 10), ("ternary", 3), ("base12", 12), ("base36", 36),
        ("octal", 8), ("hex", 16),
    ]:
        yield case, "parse", a, b, radix
    if shape_sweep:
        for denominator in (2, 3, 8, 16):
            q_short = q >> (bits - bits // denominator)
            yield f"quotient-1-{denominator}", "div", b * q_short + 42, b, 10
        for ratio in (2, 3, 4, 8, 16):
            yield f"ratio-{ratio}", "mul", a, b >> (bits - bits // ratio), 10
        q_long = q
        for ratio in (2, 3):
            # bench.plenty accepts at most one million hex digits per input.
            if (ratio + 1) * bits > 4_000_000:
                break
            q_long = (q_long << bits) | a
            yield f"quotient-{ratio}", "div", b * q_long + 42, b, 10


def measure(args, engines, path, writer):
    operands = random.Random(987)
    order = random.Random(1405)
    numerator = path / "a.hex"
    divisor = path / "b.hex"
    for bits in args.bits:
        count = max(2, min(16384, 2**21 // bits))
        for case, operation, a, b, radix in workloads(bits, operands, args.shape_sweep):
            if args.cases and f"{operation}/{case}" not in args.cases:
                continue
            numerator.write_text(f"{a:x}")
            divisor.write_text(f"{b:x}")
            checksum = None
            samples = {engine: [] for engine in engines}
            for batch in range(-1, args.batches):
                names = list(engines)
                order.shuffle(names)
                for name in names:
                    command = [
                        "taskset", "-c", str(args.cpu), engines[name],
                        operation, f"@{numerator}", f"@{divisor}", str(count), str(radix),
                    ]
                    elapsed, actual = map(int, subprocess.check_output(command, text=True).split())
                    if checksum is None:
                        checksum = actual
                    if checksum != actual:
                        raise ValueError((bits, case, operation, name, checksum, actual))
                    if batch >= 0:
                        samples[name].append(elapsed / count)
                        writer.writerow([bits, operation, case, name, batch, count, elapsed, actual])
            medians = {name: round(statistics.median(times) / 1e6, 4) for name, times in samples.items()}
            print(bits, operation, case, medians, flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("engines", nargs="+", metavar="LABEL=EXECUTABLE")
    parser.add_argument("--cpu", type=int, default=min(os.sched_getaffinity(0)))
    parser.add_argument("--batches", type=int, default=7)
    parser.add_argument(
        "--bits", type=int, nargs="+",
        default=[8192, 65536, 131072, 262144, 524288, 1048576],
    )
    parser.add_argument("--cases", nargs="+", metavar="OPERATION/CASE")
    parser.add_argument("--shape-sweep", action="store_true", help="include additional division and multiplication ratios")
    args = parser.parse_args()
    engines = {name: Path(binary).resolve() for name, binary in (engine.split("=", 1) for engine in args.engines)}
    with tempfile.TemporaryDirectory() as work, args.output.open("w", newline="") as output:
        writer = csv.writer(output)
        writer.writerow(["bits", "operation", "case", "engine", "batch", "count", "nanoseconds", "checksum"])
        measure(args, engines, Path(work), writer)


if __name__ == "__main__":
    main()
