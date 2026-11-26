"""Compare all prototype modes with exact Python products, including buffer reuse."""
import argparse
import random
import subprocess
import tempfile
from pathlib import Path


def check(binary):
    rng = random.Random(987)
    widths = (
        1, 63, 64, 65, 91, 111, 127, 128, 129, 255, 320, 511, 4096, 65536,
        131072, 196608, 262144, 393216, 524288, 720960, 786432, 1048576,
        1179648, 1572864, 3145728, 3670080,
    )
    checked = 0
    with tempfile.TemporaryDirectory() as work:
        a_path, b_path = Path(work, "a"), Path(work, "b")
        for width in widths:
            for shape, ratio in (("dense", 1), ("maximum", 1), ("dense", 3), ("sparse", 16)):
                short_width = max(1, width // ratio)
                if shape == "maximum":
                    a, b = ((1 << bits) - 1 for bits in (width, short_width))
                elif shape == "sparse":
                    a, b = ((1 << (bits - 1)) | 1 for bits in (width, short_width))
                else:
                    a, b = (rng.getrandbits(bits - 1) | (1 << (bits - 1))
                            for bits in (width, short_width))
                a_path.write_text(f"{a:x}")
                b_path.write_text(f"{b:x}")
                expected = f"{a * b:x}"
                for mode in ("flat", "blocked", "packed", "truncated"):
                    actual = subprocess.check_output(
                        [binary, mode, a_path, b_path, "2", "exact"], text=True
                    ).strip()
                    assert actual == expected, (width, shape, ratio, mode, actual[:64], expected[:64])
                    checked += 1
    print(f"{checked} exact products verified")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=lambda path: Path(path).resolve())
    check(parser.parse_args().binary)
