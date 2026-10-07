"""Compare format DLL results and scan time without modifying MLP samples."""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import random
import time


class Inspection(c.Structure):
    _pack_ = 1
    _fields_ = [("size", c.c_uint64), ("units", c.c_uint32),
                ("majors", c.c_uint32), ("interval", c.c_double)] + [
        (name, c.c_int32) for name in (
            "major_errors", "parity_errors", "substream_errors", "eos",
            "peak", "extended", "rate", "valid", "error")]


class Alignment(c.Structure):
    _pack_ = 1
    _fields_ = [(name, c.c_int32) for name in (
        "peak", "extended", "checksum", "eos", "old", "new")]


class Formats:
    def __init__(self, path):
        self.library = c.CDLL(str(path.resolve()))
        self.inspect = self.library.dvda_formats_mlp_inspect_file
        self.inspect.argtypes = [c.c_char_p, c.POINTER(Inspection)]
        self.align = self.library.dvda_formats_mlp_align_buffer
        self.align.argtypes = [c.c_void_p, c.c_size_t, c.POINTER(c.c_void_p),
                              c.POINTER(c.c_size_t), c.POINTER(Alignment)]
        self.free = self.library.dvda_formats_free
        self.free.argtypes = [c.c_void_p]

    def checksum(self, data, bits):
        value = (c.c_uint8 if bits == 8 else c.c_uint16)()
        function = getattr(self.library, f"dvda_formats_checksum{bits}")
        function.argtypes = [c.c_void_p, c.c_size_t, c.c_void_p]
        status = function(c.create_string_buffer(data), len(data), c.byref(value))
        return status, value.value

    def inspect_file(self, path):
        result = Inspection()
        started = time.perf_counter()
        status = self.inspect(str(path).encode("utf-8"), c.byref(result))
        elapsed = time.perf_counter() - started
        return status, bytes(result), elapsed

    def align_buffer(self, data):
        pointer, size, result = c.c_void_p(), c.c_size_t(), Alignment()
        status = self.align(c.create_string_buffer(data), len(data),
                            c.byref(pointer), c.byref(size), c.byref(result))
        try:
            output = c.string_at(pointer, size.value) if pointer else b""
            return status, bytes(result), output
        finally:
            self.free(pointer)


def reference_checksum(data, bits):
    """Original polynomial definition, independent of the optimized tables."""
    polynomial, initial, excluded = (0x63, 0x3c, 1) if bits == 8 else (0x2d, 0, 2)
    table = []
    for index in range(256):
        value = index << 24
        for _ in range(8):
            value = ((value << 1) ^ (polynomial << (32 - bits)
                     if value >> 31 else 0)) & 0xffffffff
        table.append(int.from_bytes(value.to_bytes(4, "big"), "little"))
    crc = initial
    for byte in data[:-excluded]:
        crc = table[(crc & 255) ^ byte] ^ (crc >> 8)
    tail = data[-1] if bits == 8 else int.from_bytes(data[-2:], "little")
    return (crc ^ tail) & ((1 << bits) - 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--samples", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--align-samples", type=int, default=8)
    args = parser.parse_args()
    before, after = Formats(args.before), Formats(args.after)
    rng = random.Random(20261007)
    checksums = 0
    for bits in (8, 16):
        minimum = 1 if bits == 8 else 2
        for size in (minimum, 3, 4, 25, 26, 255, 256, 4096, 8190):
            for _ in range(8):
                data = rng.randbytes(size)
                expected = (0, reference_checksum(data, bits))
                assert before.checksum(data, bits) == expected
                assert after.checksum(data, bits) == expected
                checksums += 1
        for size in range(minimum):
            assert before.checksum(bytes(size), bits)[0] != 0
            assert after.checksum(bytes(size), bits)[0] != 0
    paths = sorted(args.samples.rglob("*.mlp"))
    assert paths, "No MLP samples found"
    records, old_seconds, new_seconds = [], 0.0, 0.0
    for index, path in enumerate(paths):
        # Warm the same file before timing either implementation; disk cache
        # differences must not be reported as a CRC performance improvement.
        with path.open("rb") as file:
            while file.read(8 * 1024 * 1024):
                pass
        old_status, old, old_time = before.inspect_file(path)
        new_status, new, new_time = after.inspect_file(path)
        assert (old_status, old) == (new_status, new), str(path)
        assert new_status == 0 and Inspection.from_buffer_copy(new).valid, str(path)
        old_seconds += old_time
        new_seconds += new_time
        records.append({"path": str(path), "bytes": path.stat().st_size,
                        "before_seconds": old_time, "after_seconds": new_time})
        if (index + 1) % 10 == 0 or index + 1 == len(paths):
            print(f"inspect {index + 1}/{len(paths)}: "
                  f"before {old_seconds:.3f}s, after {new_seconds:.3f}s", flush=True)
    aligned = []
    # Include small and large files; compare every output byte and ABI field.
    ordered = sorted(paths, key=lambda path: path.stat().st_size)
    for path in (ordered[:args.align_samples // 2] +
                 ordered[-(args.align_samples - args.align_samples // 2):]
                 if args.align_samples else []):
        data = path.read_bytes()
        old, new = before.align_buffer(data), after.align_buffer(data)
        assert old == new and new[0] == 0, str(path)
        assert new[2] == data, f"Previously aligned sample changed: {path}"
        aligned.append({"path": str(path), "bytes": len(data),
                        "sha256": hashlib.sha256(data).hexdigest()})
    report = {"checksum_cases": checksums, "inspection_files": len(paths),
              "bytes": sum(record["bytes"] for record in records),
              "before_seconds": old_seconds, "after_seconds": new_seconds,
              "speedup": old_seconds / new_seconds,
              "inspection_results_equal": True, "alignment_results_equal": True,
              "aligned_samples": aligned, "files": records}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n",
                           encoding="utf-8")
    print(json.dumps({key: value for key, value in report.items()
                      if key not in ("files", "aligned_samples")}, indent=2))


if __name__ == "__main__":
    main()
