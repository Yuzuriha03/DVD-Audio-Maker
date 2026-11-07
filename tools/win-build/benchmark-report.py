#!/usr/bin/env python3
"""Repeat the existing ignored Rust benchmarks and emit a versioned P0 report.

The benchmark implementations stay in the Rust test suites. This driver only
builds/discovers them, runs each selected test three times, and records timing,
per-process working set, best-effort temporary-space samples and deterministic
fixture/result hashes. Optional real-file cases and ARM64 are NOT RUN.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
import datetime as dt
import glob
import hashlib
import json
import os
from pathlib import Path
import re
import statistics
import struct
import subprocess
import sys
import tempfile
import time

DEFAULT_BENCHMARKS = {
    "normalization_throughput",
    "packing_32_throughput",
    "pcm_comparison_throughput",
    "inspection_progress_callback_benchmark",
}
OPTIONAL_NOT_RUN = [
    ("release_files_benchmark", "requires DVDA_HASH_BENCH_INPUTS and DVDA_HASH_BENCH_REPORT with real files"),
    ("synthetic_batch_worker_benchmark", "requires a packaged executable and an owned benchmark root"),
    ("real_cached_mlp_preflight_benchmark_is_read_only", "requires the complete real cache manifest/root/output/media/encoder/report environment"),
]
SOURCE_FILES = {
    "normalization_throughput": "rust/crates/dvda-core/src/pcm.rs",
    "packing_32_throughput": "rust/crates/dvda-core/src/pcm.rs",
    "pcm_comparison_throughput": "rust/crates/dvda-native/src/format_parity.rs",
    "inspection_progress_callback_benchmark": "rust/crates/dvda-core/src/preparation/audio_decode_parallel.rs",
}


def sha_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def update_repeat(digest, block: bytes, count: int) -> None:
    for _ in range(count):
        digest.update(block)


def hash_normalization() -> tuple[str, str, int, int]:
    input_block = bytearray()
    output_block = bytearray()
    for value in range(65536):
        signed = value if value < 32768 else value - 65536
        input_block += struct.pack("<H", value)
        output_block += struct.pack("<i", signed << 8)[:3]
    input_data_size = len(input_block) * 256
    header = b"RIFF" + struct.pack("<I", 36 + input_data_size) + b"WAVEfmt "
    header += struct.pack("<IHHIIHH", 16, 1, 1, 48000, 96000, 2, 16)
    header += b"data" + struct.pack("<I", input_data_size)
    source = hashlib.sha256()
    source.update(header)
    update_repeat(source, bytes(input_block), 256)
    output = hashlib.sha256()
    update_repeat(output, bytes(output_block), 256)
    return source.hexdigest(), output.hexdigest(), 44 + input_data_size, len(output_block) * 256


def hash_packing() -> tuple[str, str, int, int]:
    count = 16_777_217
    data_size = count * 4
    header = b"RIFF" + struct.pack("<I", 36 + data_size) + b"WAVEfmt "
    header += struct.pack("<IHHIIHH", 16, 1, 1, 48000, 192000, 4, 32)
    header += b"data" + struct.pack("<I", data_size)
    source = hashlib.sha256()
    source.update(header)
    output = hashlib.sha256()
    for start in range(0, count, 65536):
        source_block = bytearray()
        output_block = bytearray()
        for number in range(start, min(start + 65536, count)):
            sample = (number << 8) & 0xFFFFFFFF
            source_block += struct.pack("<I", sample)
            output_block += struct.pack("<I", sample)[1:]
        source.update(source_block)
        output.update(output_block)
    return source.hexdigest(), output.hexdigest(), 44 + data_size, count * 3


def fixture_evidence(name: str, repo: Path) -> dict:
    source_path = SOURCE_FILES[name]
    evidence = {
        "kind": "deterministic_generator",
        "generator_source": source_path,
        "generator_source_sha256": sha_file(repo / source_path),
    }
    if name == "normalization_throughput":
        source, output, source_bytes, output_bytes = hash_normalization()
        evidence.update(input_sha256=source, output_sha256=output,
                        input_bytes=source_bytes, output_bytes=output_bytes,
                        output_kind="independently_computed_s24le_raw")
    elif name == "packing_32_throughput":
        source, output, source_bytes, output_bytes = hash_packing()
        evidence.update(input_sha256=source, output_sha256=output,
                        input_bytes=source_bytes, output_bytes=output_bytes,
                        output_kind="independently_computed_s24le_raw")
    elif name == "pcm_comparison_throughput":
        block = bytes(range(256))
        source = hashlib.sha256()
        update_repeat(source, block, 262144)
        source.update(b"\0")
        result = b"matches=1;reason=0;source_bytes=67108865;decoded_bytes=67108865"
        evidence.update(input_sha256=source.hexdigest(), output_sha256=sha_bytes(result),
                        input_bytes=64 * 1024 * 1024 + 1, output_bytes=len(result),
                        output_kind="canonical_comparison_result")
    else:
        jobs = "".join(f"metadata:{index}\n" for index in range(8)).encode()
        result = b"jobs=8;generated_messages=40000;final_markers=8"
        evidence.update(input_sha256=sha_bytes(jobs), output_sha256=sha_bytes(result),
                        input_bytes=len(jobs), output_bytes=len(result),
                        output_kind="canonical_progress_result")
    return evidence


class MemoryCounters(ctypes.Structure):
    _fields_ = [
        ("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD),
        ("PeakWorkingSetSize", ctypes.c_size_t), ("WorkingSetSize", ctypes.c_size_t),
        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t), ("QuotaPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t), ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
        ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t),
    ]


def process_memory(handle: int) -> tuple[int, int]:
    counters = MemoryCounters()
    counters.cb = ctypes.sizeof(counters)
    if not ctypes.windll.psapi.GetProcessMemoryInfo(
        wintypes.HANDLE(handle), ctypes.byref(counters), counters.cb
    ):
        return 0, 0
    return counters.WorkingSetSize, counters.PeakWorkingSetSize


def directory_bytes(paths: list[str]) -> int:
    total = 0
    for root in paths:
        if not os.path.isdir(root):
            continue
        for directory, _, files in os.walk(root, followlinks=False):
            for name in files:
                try:
                    total += os.path.getsize(os.path.join(directory, name))
                except OSError:
                    pass
    return total


def temporary_paths(name: str, pid: int, repo: Path) -> list[str]:
    temp = tempfile.gettempdir()
    if name == "normalization_throughput":
        return [os.path.join(temp, f"dvda-pcm-throughput-{pid}")]
    if name == "packing_32_throughput":
        return [os.path.join(temp, f"dvda-pack32-throughput-{pid}")]
    if name == "pcm_comparison_throughput":
        pattern = str(repo / "rust" / "target" / f"dvda-format-parity-*-{pid}-*")
        return glob.glob(pattern)
    return []


def run_benchmark(exe: str, name: str, round_index: int, log: Path, repo: Path) -> dict:
    command = [exe, "--ignored", "--exact", name, "--nocapture"]
    started = time.perf_counter()
    with log.open("w", encoding="utf-8", errors="replace", newline="\n") as stream:
        process = subprocess.Popen(command, cwd=repo / "rust", stdout=stream,
                                   stderr=subprocess.STDOUT)
        peak_working = 0
        peak_temporary = 0
        while process.poll() is None:
            _, reported_peak = process_memory(process._handle)
            peak_working = max(peak_working, reported_peak)
            peak_temporary = max(peak_temporary,
                                 directory_bytes(temporary_paths(name.split("::")[-1], process.pid, repo)))
            time.sleep(0.05)
        _, reported_peak = process_memory(process._handle)
        peak_working = max(peak_working, reported_peak)
        peak_temporary = max(peak_temporary,
                             directory_bytes(temporary_paths(name.split("::")[-1], process.pid, repo)))
        return_code = process.returncode
    elapsed_ms = (time.perf_counter() - started) * 1000
    text = log.read_text(encoding="utf-8", errors="replace")
    pcm = re.findall(r"PCM_BENCH round=(\d+) ms=([0-9.]+) bytes=(\d+)", text)
    compare = re.findall(r"COMPARE_BENCH round=(\d+) ms=([0-9.]+)", text)
    progress = re.search(r"PROGRESS_BENCH seconds=([0-9.]+) messages=(\d+) jobs=8 generated=40000", text)
    samples = [float(match[1]) for match in pcm]
    samples += [float(match[1]) for match in compare]
    if progress:
        samples = [float(progress.group(1)) * 1000]
    metric_events = []
    for line in text.splitlines():
        if line.startswith("[METRIC] "):
            try:
                metric_events.append(json.loads(line[len("[METRIC] "):]))
            except json.JSONDecodeError:
                pass
    worker_peaks = [event.get("active_workers_peak") for event in metric_events
                    if event.get("event") == "finish" and event.get("active_workers_peak") is not None]
    return {
        "round": round_index,
        "status": "PASS" if return_code == 0 else "FAIL",
        "return_code": return_code,
        "elapsed_ms": statistics.median(samples) if samples else elapsed_ms,
        "timing_source": "test-reported median" if samples else "process wall clock",
        "samples_ms": samples,
        "process_wall_ms": elapsed_ms,
        "process_peak_working_set_bytes": peak_working or None,
        "worker_peak": max(worker_peaks) if worker_peaks else None,
        "worker_peak_note": None if worker_peaks else "not emitted by this test",
        "temporary_space": {
            "sampled_peak_bytes": peak_temporary,
            "sampling_interval_ms": 50,
            "scope": "known benchmark-owned temporary roots only; short-lived files may be missed",
        },
        "metric_events": metric_events,
        "command": command,
        "log": str(log),
    }


def discover(repo: Path) -> dict[str, list[str]]:
    command = [
        "powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
        str(repo / "tools" / "win-build" / "cargo-msvc.ps1"), "test",
        "--manifest-path", "rust/Cargo.toml", "--target", "x86_64-pc-windows-msvc",
        "--release", "--offline", "-p", "dvda-core", "-p", "dvda-native",
        "--no-run", "--message-format=json",
    ]
    completed = subprocess.run(command, cwd=repo, text=True, encoding="utf-8",
                               errors="replace", stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT)
    if completed.returncode:
        raise RuntimeError("benchmark discovery failed:\n" + completed.stdout[-8000:])
    executables = set()
    for line in completed.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") == "compiler-artifact" and message.get("profile", {}).get("test") and message.get("executable"):
            executables.add(message["executable"])
    discovered = {name: [] for name in DEFAULT_BENCHMARKS}
    for executable in sorted(executables):
        listed = subprocess.run([executable, "--list", "--ignored"], cwd=repo / "rust",
                                text=True, encoding="utf-8", errors="replace",
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        if listed.returncode:
            raise RuntimeError(f"test listing failed: {executable}\n{listed.stdout}")
        for line in listed.stdout.splitlines():
            match = re.match(r"^(.+): test$", line)
            if match and match.group(1).split("::")[-1] in DEFAULT_BENCHMARKS:
                discovered[match.group(1).split("::")[-1]].append(match.group(1))
    missing = [name for name, tests in discovered.items() if not tests]
    if missing:
        raise RuntimeError("missing benchmark tests: " + ", ".join(missing))
    return {"executables": sorted(executables), "tests": discovered}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--output", type=Path, default=Path("build/benchmarks/baseline-x64.json"))
    parser.add_argument("--rounds", type=int, default=3)
    args = parser.parse_args()
    if args.rounds != 3:
        parser.error("P0 requires exactly three rounds (--rounds 3)")
    repo = args.repo.resolve()
    output = (repo / args.output).resolve() if not args.output.is_absolute() else args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    log_root = output.parent / (output.stem + "-logs")
    log_root.mkdir(exist_ok=True)
    commit = subprocess.check_output(["git", "-c", "safe.directory=E:/yyz57/Documents/Visual Studio Code/DVD-Audio-Maker", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
    discovery = discover(repo)
    benchmarks = []
    failed = False
    for name in sorted(DEFAULT_BENCHMARKS):
        evidence = fixture_evidence(name, repo)
        rounds = []
        for round_index in range(1, args.rounds + 1):
            for full_name in discovery["tests"][name]:
                exe = next(exe for exe in discovery["executables"]
                           if subprocess.run([exe, "--list", "--ignored"], cwd=repo / "rust",
                                             text=True, encoding="utf-8", errors="replace",
                                             stdout=subprocess.PIPE).stdout.find(full_name + ": test") >= 0)
                rounds.append(run_benchmark(exe, full_name, round_index,
                                            log_root / f"{name}-{round_index}.log", repo))
        failed = failed or any(item["status"] != "PASS" for item in rounds)
        benchmarks.append({
            "name": name,
            "status": "FAIL" if any(item["status"] != "PASS" for item in rounds) else "PASS",
            "input": evidence,
            "rounds": rounds,
            "median_elapsed_ms": statistics.median(item["elapsed_ms"] for item in rounds),
            "output_sha256": evidence["output_sha256"],
        })
    not_run = [{"name": name, "reason": reason} for name, reason in OPTIONAL_NOT_RUN]
    not_run.append({
        "name": "aarch64-pc-windows-msvc",
        "reason": "native ARM64 execution is not available in this x64 session; x64 is the required measured target",
    })
    report = {
        "version": 1,
        "created_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "commit": commit,
        "target": "x86_64-pc-windows-msvc",
        "runner": {
            "script": "tools/win-build/benchmark-report.py",
            "rounds_per_benchmark": args.rounds,
            "aggregation": "median of the three per-run reported medians/timings",
            "discovery": discovery,
        },
        "benchmarks": benchmarks,
        "not_run": not_run,
        "limitations": [
            "Working set is the per-test process peak, not the total workflow or system peak.",
            "Temporary space is sampled only at known benchmark-owned roots and can miss short-lived files.",
            "Generated fixture and expected-output hashes are computed independently; the Rust tests verify those outputs internally.",
            "Optional real-file cases and ARM64 are NOT RUN, never counted as passes.",
        ],
    }
    output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(output), "benchmarks": [
        {"name": item["name"], "status": item["status"], "median_elapsed_ms": item["median_elapsed_ms"]}
        for item in benchmarks], "not_run": not_run}, ensure_ascii=False, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
