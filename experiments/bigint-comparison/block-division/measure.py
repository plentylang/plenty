"""Compare optimized benchmark executables using file-backed operands."""
import argparse
import csv
import os
import random
import statistics
import subprocess
import tempfile
from pathlib import Path


def workloads(bits, rng):
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


def measure(args, engines, path, writer):
    operands = random.Random(987)
    order = random.Random(1405)
    numerator = path / "a.hex"
    divisor = path / "b.hex"
    for bits in args.bits:
        count = max(2, min(16384, 2**21 // bits))
        for case, operation, a, b, radix in workloads(bits, operands):
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
    args = parser.parse_args()
    engines = {name: Path(binary).resolve() for name, binary in (engine.split("=", 1) for engine in args.engines)}
    with tempfile.TemporaryDirectory() as work, args.output.open("w", newline="") as output:
        writer = csv.writer(output)
        writer.writerow(["bits", "operation", "case", "engine", "batch", "count", "nanoseconds", "checksum"])
        measure(args, engines, Path(work), writer)


if __name__ == "__main__":
    main()
