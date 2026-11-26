"""Time full unsigned products from interleaved, CPU-pinned optimized builds."""
import argparse
import csv
import random
import re
import statistics
import subprocess
import tempfile
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('output', type=Path)
parser.add_argument('--cpu', type=int, default=10)
parser.add_argument('--batches', type=int, default=7)
parser.add_argument('--min-count', type=int, default=2, help='minimum products per timing sample')
parser.add_argument('--bits', type=int, nargs='+', default=[65536, 131072, 262144, 393216, 524288, 720960, 786432, 1048576, 1572864, 3145728])
parser.add_argument('--plenty', type=lambda path: Path(path).resolve(), required=True)
parser.add_argument('--malachite', type=lambda path: Path(path).resolve(), required=True)
parser.add_argument('--vector', type=lambda path: Path(path).resolve(), required=True)
parser.add_argument('--modes', nargs='+', choices=['flat', 'blocked', 'packed', 'truncated'],
                    default=['flat', 'blocked', 'packed', 'truncated'])
args = parser.parse_args()
if min(args.batches, args.min_count) < 1 or any(width < 3 for width in args.bits):
    parser.error('use positive batches and counts, and operand widths of at least three bits')
rng, order = random.Random(987), random.Random(1419)

with tempfile.TemporaryDirectory() as tmp, args.output.open('w', newline='') as output:
    writer = csv.writer(output)
    writer.writerow(['bits', 'ratio', 'engine', 'batch', 'count', 'elapsed_ns', 'plan_ns', 'profile'])
    a_path, b_path = Path(tmp, 'a'), Path(tmp, 'b')
    prefix = ['taskset', '-c', str(args.cpu)]
    for width in args.bits:
        for ratio in (1, 3):
            a, b = (rng.getrandbits(n - 1) | (1 << (n - 1)) for n in (width, width // ratio))
            count = max(args.min_count, 2**21 // width)
            a_path.write_text(f'{a:x}')
            b_path.write_text(f'{b:x}')
            product = a * b
            expected = product.bit_length() * count
            for mode in args.modes:
                actual = subprocess.check_output(
                    [*prefix, args.vector, mode, a_path, b_path, '1', 'exact'], text=True
                ).strip()
                assert actual == f'{product:x}', (mode, width, ratio)
            samples = {mode: [] for mode in ['plenty', 'malachite', *args.modes]}
            setups = {mode: [] for mode in args.modes}
            profiles = set()
            for batch in range(-1, args.batches):
                modes = list(samples)
                order.shuffle(modes)
                for mode in modes:
                    setup = None
                    profile = None
                    if mode in ('plenty', 'malachite'):
                        binary = args.plenty if mode == 'plenty' else args.malachite
                        proc = subprocess.run([*prefix, binary, 'mul', '@' + str(a_path), '@' + str(b_path), str(count)],
                                              check=True, text=True, capture_output=True)
                    else:
                        proc = subprocess.run([*prefix, args.vector, mode, a_path, b_path, str(count)],
                                              check=True, text=True, capture_output=True)
                        desc = re.fullmatch(r'(.+), plan=(\d+) ns\n', proc.stderr)
                        profile, setup = desc[1], int(desc[2])
                        profiles.add(profile)
                        if batch >= 0:
                            setups[mode].append(setup)
                    elapsed, checksum = map(int, proc.stdout.split())
                    assert checksum == expected, (mode, width, ratio, checksum, expected)
                    if batch >= 0:
                        samples[mode].append(elapsed / count)
                        writer.writerow([width, ratio, mode, batch, count, elapsed, setup, profile])
            median = {mode: round(statistics.median(values) / 1e6, 4) for mode, values in samples.items()}
            assert len(profiles) == 1, profiles
            plans = {mode: round(statistics.median(values) / 1e6, 4) for mode, values in setups.items()}
            print(width, ratio, profiles.pop(), median,
                  'plan', plans, flush=True)
