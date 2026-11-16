"""Validate a single-EXE Windows release directory.

Checks that release-build.json and MANIFEST.txt describe exactly the published
EXE, that the EXE SHA-256 and PE architecture match the recorded target, that
every recorded input is traceable, and that every native asset embedded in its
DVDRUN01 runtime is intact.

This developer check needs only Python and the Windows Cabinet API; it never
starts a disc build, deletes files, or performs a formal release. Unrelated
files are reported, never removed, so an old ZIP or a stale EXE can never be
mistaken for the current release asset.
"""
import argparse
import ctypes as c
import hashlib
import json
import struct
from pathlib import Path


TARGETS = {
    "Windows x64": ("win-x64", "x86_64-pc-windows-msvc", 0x8664),
    "Windows ARM64": ("win-arm64", "aarch64-pc-windows-msvc", 0xAA64),
}
RECORD_FILES = ("MANIFEST.txt", "release-build.json")
INPUT_KEYS = (
    "target_directory", "media_runtime", "image_runtime", "image_author",
    "ffmpeg_prefix", "prebuilt", "source",
)
FONT_PATH = "fonts/DvdaNotoCJK-Regular.ttc"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def release_exe_name(record, suffix):
    version = record.get("version")
    assert isinstance(version, str) and version.startswith("v"), (
        f"release-build.json has an invalid version: {version!r}")
    return f"DVD-Audio-Maker-{version}-{suffix}.exe"


def runtime(executable):
    position = 0
    while True:
        position = executable.find(b"DVDRUN01", position)
        assert position >= 0, "Embedded runtime metadata is missing"
        try:
            size = struct.unpack_from("<I", executable, position + 8)[0]
            assert size <= 1024 * 1024
            entries = json.loads(executable[position + 12:position + 12 + size])
            assert isinstance(entries, list) and entries and "path" in entries[0]
            break
        except (AssertionError, ValueError, KeyError, struct.error):
            position += 8
    payload = position + 12 + size
    cabinet = c.WinDLL("cabinet", use_last_error=True)
    cabinet.CreateDecompressor.argtypes = [c.c_uint32, c.c_void_p, c.POINTER(c.c_void_p)]
    cabinet.Decompress.argtypes = [c.c_void_p, c.c_void_p, c.c_size_t,
                                  c.c_void_p, c.c_size_t, c.POINTER(c.c_size_t)]
    cabinet.CloseDecompressor.argtypes = [c.c_void_p]
    handle = c.c_void_p()
    assert cabinet.CreateDecompressor(4, None, c.byref(handle)), c.get_last_error()
    hashes = {}
    try:
        for entry in entries:
            assert 0 < entry["size"] <= 128 * 1024 * 1024
            start = payload + entry["offset"]
            data = executable[start:start + entry["length"]]
            assert len(data) == entry["length"]
            buffer = c.create_string_buffer(entry["size"])
            written = c.c_size_t()
            assert cabinet.Decompress(handle, data, len(data), buffer,
                                      entry["size"], c.byref(written)), c.get_last_error()
            assert written.value == entry["size"]
            digest = sha(buffer.raw)
            assert digest == entry["sha256"].lower(), entry["path"]
            assert entry["path"] not in hashes, entry["path"]
            hashes[entry["path"]] = digest
    finally:
        cabinet.CloseDecompressor(handle)
    assert [p for p in hashes if p.endswith((".otf", ".ttc"))] == [FONT_PATH]
    assert not any(p.endswith((".json", ".md", ".txt", ".cs")) for p in hashes)
    return hashes


def check_records(root, record, expected_files):
    """Verify the published EXE hash record and the MANIFEST.txt mirror."""
    files = record.get("files")
    assert isinstance(files, dict), "release-build.json has no file records"
    assert set(files) == expected_files, (
        f"release-build.json lists {sorted(files)}, expected {sorted(expected_files)}")
    hashes = {}
    for name, metadata in files.items():
        data = (root / name).read_bytes()
        assert metadata.get("bytes") == len(data), name
        assert metadata.get("sha256", "").lower() == sha(data), name
        hashes[name] = metadata["sha256"].lower()
    manifest = {}
    for line in (root / "MANIFEST.txt").read_text(encoding="utf-8").splitlines():
        digest, separator, name = line.partition("  ")
        assert separator and name, f"Malformed MANIFEST.txt line: {line!r}"
        assert name not in manifest, f"Duplicate MANIFEST.txt record: {name}"
        assert sha((root / name).read_bytes()) == digest.lower(), name
        manifest[name] = digest.lower()
    assert manifest == hashes, (
        f"MANIFEST.txt records {sorted(manifest)} but release-build.json lists {sorted(hashes)}")
    return hashes


def check_inputs(record, target_triple):
    """Every architecture input must be explicit and traceable."""
    inputs = record.get("inputs")
    assert isinstance(inputs, dict), "release-build.json has no input records"
    for key in INPUT_KEYS:
        value = inputs.get(key)
        assert isinstance(value, str) and value, f"Missing recorded release input: {key}"
        assert Path(value).is_absolute(), f"Release input is not absolute: {key}={value!r}"
    assert (Path(inputs["prebuilt"]) / "fonts").is_dir(), (
        f"Recorded --prebuilt does not contain fonts/: {inputs['prebuilt']}")
    assert (Path(inputs["source"]) / "menu").is_dir(), (
        f"Recorded --source does not contain menu/: {inputs['source']}")
    environment = record.get("build_environment")
    assert isinstance(environment, dict), "release-build.json has no build environment record"
    linkage = environment.get("ffmpeg_linkage")
    assert linkage in ("static", "shared"), f"Unknown FFmpeg linkage: {linkage!r}"
    prefix = environment.get("DVDA_FFMPEG_PREFIX")
    assert isinstance(prefix, str) and prefix, (
        "GUI release build must record the validated DVDA_FFMPEG_PREFIX")
    assert prefix == inputs["ffmpeg_prefix"], (
        "DVDA_FFMPEG_PREFIX must be the recorded release input")
    manifest_path = Path(prefix) / "build-manifest.json"
    assert manifest_path.is_file(), f"Missing FFmpeg build manifest: {manifest_path}"
    ffmpeg = json.loads(manifest_path.read_text(encoding="utf-8-sig"))
    assert ffmpeg.get("target") == target_triple, (
        f"FFmpeg target {ffmpeg.get('target')!r} does not match {target_triple}")
    assert ffmpeg.get("toolchain") == "msvc", "FFmpeg prefix is not an MSVC build"
    assert ffmpeg.get("linkage") == linkage, (
        f"FFmpeg prefix linkage {ffmpeg.get('linkage')!r} does not match {linkage!r}")
    if linkage == "static":
        assert prefix == inputs["media_runtime"], (
            "Static FFmpeg must use the validated media runtime as its prefix")
        assert ffmpeg.get("profile") == "static", "Static FFmpeg prefix is not the static profile"
        assert ffmpeg.get("crt") == "static", "Static FFmpeg prefix does not use the static CRT"
    else:
        assert ffmpeg.get("profile") == "shared", (
            "Shared FFmpeg prefix does not record profile=shared")
        for name, version in (
            ("avformat", 63), ("avcodec", 63), ("avutil", 61),
            ("swresample", 7), ("swscale", 10),
        ):
            definition = Path(prefix) / "lib" / f"{name}-{version}.def"
            assert definition.is_file(), f"Shared FFmpeg prefix lacks {definition.name}"
        for header in ("libavcodec/avcodec.h", "libavformat/avformat.h"):
            assert (Path(prefix) / "include" / header).is_file(), (
                f"Shared FFmpeg prefix lacks {header}")
    assert environment.get("RUSTFLAGS") is None, (
        "GUI release build must not inherit RUSTFLAGS")
    return inputs, environment


def check(root):
    root = root.resolve()
    record = json.loads((root / "release-build.json").read_text(encoding="utf-8-sig"))
    target_name = record.get("target")
    assert target_name in TARGETS, f"Unknown release target: {target_name!r}"
    suffix, target_triple, expected_machine = TARGETS[target_name]
    assert record.get("target_triple") == target_triple, (
        f"Recorded target triple {record.get('target_triple')!r} does not match {target_triple}")
    assert record.get("single_exe") is True, (
        "release-build.json does not describe the current single-EXE release")
    exe_name = release_exe_name(record, suffix)
    expected_files = {exe_name}
    hashes = check_records(root, record, expected_files)
    inputs, environment = check_inputs(record, target_triple)
    component_manifests = record.get("component_manifests")
    assert isinstance(component_manifests, list) and component_manifests, (
        "release-build.json has no component manifest records")
    for entry in component_manifests:
        path = Path(entry["path"])
        assert path.is_file(), f"Recorded component manifest is missing: {path}"
        assert sha(path.read_bytes()) == entry["sha256"].lower(), str(path)
    executable = (root / exe_name).read_bytes()
    pe_offset = struct.unpack_from("<I", executable, 0x3C)[0]
    assert executable[pe_offset:pe_offset + 4] == b"PE\0\0", "Shipped EXE has no PE header"
    machine = struct.unpack_from("<H", executable, pe_offset + 4)[0]
    assert machine == expected_machine, (
        f"Shipped EXE machine 0x{machine:04X} does not match target {target_name}")
    runtime_hashes = runtime(executable)
    assert not any(p.is_dir() and p.name.startswith(".package-")
                   for p in root.iterdir()), "Temporary package candidates remain"
    unrelated = sorted(
        p.name for p in root.iterdir()
        if p.is_file() and p.name not in expected_files and p.name not in RECORD_FILES)
    return {
        "directory": str(root),
        "target": target_name,
        "target_triple": target_triple,
        "exe": exe_name,
        "exe_sha256": hashes[exe_name],
        "manifest_records": len(hashes),
        "embedded_files": len(runtime_hashes),
        "runtime_hashes": runtime_hashes,
        "inputs": inputs,
        "build_environment": environment,
        "unrelated_files": unrelated,
        "temporary_candidates_remaining": 0,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directories", nargs="+", type=Path)
    parser.add_argument("--output", type=Path,
                        default=Path("build/rust-packaging-migration-validation.json"))
    args = parser.parse_args()
    result = {"passed": True, "cases": [check(root) for root in args.directories]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    for case in result["cases"]:
        print(f"PASS {case['directory']}: {case['exe']} "
              f"(sha256 {case['exe_sha256'][:16]}…, {case['embedded_files']} embedded files, "
              f"{case['manifest_records']} manifest record)")
        if case["unrelated_files"]:
            print(f"  unrelated files (not published, left untouched): "
                  f"{', '.join(case['unrelated_files'])}")


if __name__ == "__main__":
    main()
