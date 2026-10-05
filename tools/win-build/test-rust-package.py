"""Check ZIP layout, external provenance/hashes and every embedded native asset.

This developer check needs only Python and Windows Cabinet; it never starts a
disc build and its output record remains outside the user release ZIP.
"""
import argparse
import ctypes as c
import hashlib
import json
import struct
import zipfile
from pathlib import Path


EXPECTED = {
    "DVD-Audio-Maker.exe", "README.md", "README.en.md", "README.ja.md",
    "LICENSE", "THIRD-PARTY.md", "THIRD-PARTY.en.md",
    "NOTICE-Image.txt", "NOTICE-Menu.txt",
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


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
    assert [p for p in hashes if p.endswith((".otf", ".ttc"))] == [
        "fonts/DvdaNotoCJK-Regular.ttc"]
    assert not any(p.endswith((".json", ".md", ".txt", ".cs")) for p in hashes)
    return hashes


def check(root):
    root = root.resolve()
    record = json.loads((root / "release-build.json").read_text(encoding="utf-8"))
    archive_name = f"DVD-Audio-Maker-{record['version']}-win-x64.zip"
    archive = root / archive_name
    expected_records = {"DVD-Audio-Maker/" + name for name in EXPECTED} | {archive_name}
    assert set(record["files"]) == expected_records
    for name, metadata in record["files"].items():
        data = (root / name).read_bytes()
        assert len(data) == metadata["bytes"], name
        assert sha(data) == metadata["sha256"], name
    manifest = {}
    for line in (root / "MANIFEST.txt").read_text(encoding="utf-8").splitlines():
        digest, name = line.split("  ", 1)
        assert name not in manifest
        assert sha((root / name).read_bytes()) == digest, name
        manifest[name] = digest
    assert set(manifest) == expected_records
    for entry in record["component_manifests"]:
        assert sha(Path(entry["path"]).read_bytes()) == entry["sha256"], entry["path"]
    assert {p.name for p in (root / "DVD-Audio-Maker").iterdir()} == EXPECTED
    with zipfile.ZipFile(archive) as zipped:
        assert len(zipped.namelist()) == len(EXPECTED)
        assert set(zipped.namelist()) == EXPECTED, zipped.namelist()
        assert zipped.testzip() is None
        for name in EXPECTED:
            assert zipped.read(name) == (root / "DVD-Audio-Maker" / name).read_bytes(), name
    executable = (root / "DVD-Audio-Maker/DVD-Audio-Maker.exe").read_bytes()
    hashes = runtime(executable)
    assert not list(root.glob(".package-*")), "Temporary package candidates remain"
    return {"directory": str(root), "zip": archive_name,
            "zip_sha256": sha(archive.read_bytes()), "exe_sha256": sha(executable),
            "zip_root_files": sorted(EXPECTED), "manifest_records": len(manifest),
            "embedded_files": len(hashes), "runtime_hashes": hashes,
            "temporary_candidates_remaining": 0}


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
        print(f"PASS {case['directory']}: {len(case['zip_root_files'])} ZIP files, "
              f"{case['embedded_files']} verified runtime files, {case['manifest_records']} hashes")


if __name__ == "__main__":
    main()
