"""Validate the Rust ISO writer with independent ISO9660/UDF readers.

Checks descriptor CRCs, shared extents, DVD-Audio placement, directory boundaries,
Unicode and failure preservation. No C author source or oracle is compiled.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile

import pycdlib

ROOT = Path(__file__).resolve().parents[2]
SECTOR = 2048


def little(data, offset, size=4):
    return int.from_bytes(data[offset:offset+size], 'little')


def assert_udf_tag(data, expected_id, expected_location):
    assert little(data, 0, 2) == expected_id
    assert sum(data[:4]+data[5:16]) & 255 == data[4], 'UDF tag checksum'
    length = little(data, 10, 2)
    crc = 0
    for value in data[16:16+length]:
        crc ^= value << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) & 0xffff if crc & 0x8000 else (crc << 1) & 0xffff
    assert crc == little(data, 8, 2), 'UDF descriptor CRC'
    assert little(data, 12) == expected_location, 'UDF descriptor location'
    return data


def inspect_layout(image_path, contents, expected_order):
    image = image_path.read_bytes()

    def tag(lba, identifier, location):
        return assert_udf_tag(image[lba*SECTOR:(lba+1)*SECTOR], identifier, location)

    def iso_entries(lba, size):
        data = image[lba*SECTOR:lba*SECTOR+size]
        result, offset = {}, 0
        while offset < len(data):
            length = data[offset]
            if length == 0:
                offset = (offset//SECTOR+1)*SECTOR
                continue
            assert offset % SECTOR + length <= SECTOR, 'ISO record crosses a sector'
            record = data[offset:offset+length]
            name = record[33:33+record[32]].decode('ascii')
            if name not in ('\0', '\1'):
                result[name.removesuffix(';1')] = little(record, 2), little(record, 10)
            offset += length
        return result

    def udf_entries(lba, size, partition):
        data = image[lba*SECTOR:lba*SECTOR+size]
        result, offset = {}, 0
        while offset < len(data):
            length = (38+data[offset+19]+3) & ~3
            record = data[offset:offset+length]
            assert_udf_tag(record, 257, lba-partition+offset//SECTOR)
            if not record[18] & 8:
                name = record[38:38+record[19]]
                assert name[0] == 8
                result[name[1:].decode('ascii')] = partition+little(record, 24)
            offset += length
        return result

    pvd = image[16*SECTOR:17*SECTOR]
    assert pvd[:7] == b'\x01CD001\x01'
    assert little(pvd, 80) == len(image)//SECTOR
    assert pvd[181] == 2 and pvd[881] == 1
    assert little(pvd, 184, 2) == int.from_bytes(pvd[186:188], 'big') == 1
    root = iso_entries(little(pvd, 158), little(pvd, 166))
    audio = iso_entries(*root['AUDIO_TS'])
    assert list(audio) == sorted(expected_order), 'ISO directory names are alphabetic'
    assert sorted(audio, key=lambda name: audio[name][0]) == expected_order, 'DVD-Audio physical placement'
    for name, payload in contents.items():
        lba, size = audio[name]
        assert size == len(payload)
        assert image[lba*SECTOR:lba*SECTOR+size] == payload
    for lba, identifier in [(18, b'BEA01'), (19, b'NSR02'), (20, b'TEA01')]:
        assert image[lba*SECTOR+1:lba*SECTOR+6] == identifier
    anchor = tag(256, 2, 256)
    assert (little(anchor, 16), little(anchor, 20)) == (16*SECTOR, 32)
    assert (little(anchor, 24), little(anchor, 28)) == (16*SECTOR, 48)
    descriptors = [tag(32+rank, identifier, 32+rank)
                   for rank, identifier in enumerate([1, 4, 5, 6, 7, 8])]
    for rank, identifier in enumerate([1, 4, 5, 6, 7, 8]):
        tag(48+rank, identifier, 48+rank)
    integrity = tag(64, 9, 64)
    tag(65, 8, 65)
    end = len(image)//SECTOR-1
    end_anchor = tag(end, 2, end)
    assert (little(end_anchor, 20), little(end_anchor, 28)) == (32, 48)
    partition = little(descriptors[2], 188)
    assert partition == 257 and little(descriptors[2], 192) == end-partition
    assert little(descriptors[3], 436) == 64
    fsd = tag(partition, 256, 0)
    tag(partition+1, 8, 1)
    root_fe_lba = partition+little(fsd, 404)
    root_fe = tag(root_fe_lba, 261, root_fe_lba-partition)
    root_udf = udf_entries(partition+little(root_fe, 180), little(root_fe, 176), partition)
    audio_fe_lba = root_udf['AUDIO_TS']
    audio_fe = tag(audio_fe_lba, 261, audio_fe_lba-partition)
    audio_udf = udf_entries(partition+little(audio_fe, 180), little(audio_fe, 176), partition)
    for name, payload in contents.items():
        file_lba = audio_udf[name]
        entry = tag(file_lba, 261, file_lba-partition)
        assert little(entry, 56, 8) == len(payload)
        extent = partition+little(entry, 180)
        assert extent == audio[name][0], 'ISO9660/UDF use the same file extent'
        assert image[extent*SECTOR:extent*SECTOR+len(payload)] == payload
    assert little(integrity, 120) == len(contents) and little(integrity, 124) == 2


def run_writer(writer, source, destination, label, success=True):
    result = subprocess.run([str(writer), str(source), str(destination), label],
                            capture_output=True, text=True, encoding="utf-8")
    if success and result.returncode:
        raise AssertionError(result.stderr)
    if not success and not result.returncode:
        raise AssertionError("Writer unexpectedly accepted invalid input")


def traverse(image, source):
    mounted = pycdlib.PyCdlib()
    mounted.open(str(image))
    try:
        expected = {p.relative_to(source).as_posix(): p.read_bytes()
                    for p in source.rglob("*") if p.is_file()}
        for filesystem in ("iso", "udf"):
            actual = {}
            for directory, _, files in mounted.walk(**{filesystem + "_path": "/"}):
                for name in files:
                    path = directory.rstrip("/") + "/" + name
                    output = io.BytesIO()
                    mounted.get_file_from_iso_fp(output, **{filesystem + "_path": path})
                    actual[path.lstrip("/").removesuffix(";1")] = output.getvalue()
            assert actual == expected, (filesystem, set(actual) ^ set(expected))
    finally:
        mounted.close()


def inspect_unicode_udf(image, source):
    mounted = pycdlib.PyCdlib()
    mounted.open(str(image))
    try:
        for path in source.rglob("*"):
            if path.is_file():
                output = io.BytesIO()
                mounted.get_file_from_iso_fp(output, udf_path="/" + path.relative_to(source).as_posix())
                assert output.getvalue() == path.read_bytes()
    finally:
        mounted.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--writer", type=Path, help="Rust iso-write executable; otherwise build it")
    parser.add_argument("--disc", type=Path, help="Also read an existing production Rust-authored disc tree")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    writer = args.writer
    if writer is None:
        subprocess.run(["cargo", "build", "--manifest-path", str(ROOT / "rust/Cargo.toml"),
                        "--offline", "-p", "dvda-author", "--example", "iso-write"], check=True)
        writer = ROOT / "rust/target/debug/examples/iso-write.exe"
    writer = writer.resolve()
    report = {"implementation": "rust-iso9660-udf102", "project_c_compiled": False,
              "writer_sha256": hashlib.sha256(writer.read_bytes()).hexdigest(),
              "fixtures": [], "reader": [], "failure": []}
    with tempfile.TemporaryDirectory(prefix="dvda-rust-iso-") as temporary:
        work = Path(temporary)
        source = work / "source"
        (source / "AUDIO_TS").mkdir(parents=True)
        (source / "VIDEO_TS").mkdir()
        (source / "EXTRAS").mkdir()
        (source / "EMPTY").mkdir()
        names = ["AUDIO_PP.IFO", "AUDIO_TS.IFO", "AUDIO_TS.VOB", "AUDIO_TS.BUP",
                 "AUDIO_SV.IFO", "AUDIO_SV.VOB", "AUDIO_SV.BUP", "ATS_01_0.IFO",
                 "ATS_01_1.AOB", "ATS_01_2.AOB", "ATS_01_0.BUP", "ATS_02_0.IFO",
                 "ATS_02_1.AOB", "ATS_02_0.BUP", "UNRANKED.BIN"]
        for index, name in enumerate(reversed(names)):
            (source / "AUDIO_TS" / name).write_bytes(bytes([index]) * [0, 1, 2047, 2048, 2049, 65537][index % 6])
        (source / "EXTRAS/FILE.BIN").write_bytes(bytes(range(256)) * 300)
        (source / "README.TXT").write_bytes(b"root file")
        fixtures = [("synthetic", source), ("empty", source / "EMPTY")]
        if args.disc is not None:
            if not args.disc.is_dir():
                raise AssertionError('Production disc tree is missing: '+str(args.disc))
            fixtures.append(("production-disc", args.disc.resolve()))
        for fixture, tree in fixtures:
            for label in ["DVD-AUDIO", "", "Wuthering Waves Singles & EPs 1", "\u97f3\u4e50\U0001f3b5" * 20]:
                actual = work / "rust.iso"
                run_writer(writer, tree, actual, label)
                actual_bytes = actual.read_bytes()
                traverse(actual, tree)
                report["fixtures"].append({"fixture": fixture, "label": label, "bytes": len(actual_bytes),
                                         "sha256": hashlib.sha256(actual_bytes).hexdigest()})
        report["reader"].append("ISO9660 and UDF traversal/payload extraction for every fixture")
        ordered = work / "ordered"
        (ordered / "AUDIO_TS").mkdir(parents=True)
        order = names[:-1]
        contents = {name: bytes([rank+1])*(SECTOR+31+rank) for rank, name in enumerate(order)}
        for name in reversed(order):
            (ordered / 'AUDIO_TS' / name).write_bytes(contents[name])
        actual = work / 'ordered.iso'
        run_writer(writer, ordered, actual, 'DVD-AUDIO-TEST')
        inspect_layout(actual, contents, order)
        traverse(actual, ordered)
        report["reader"].append("DVD-Audio physical ordering, PVD fields, UDF descriptor CRCs and shared file extents")
        # Records spanning sectors must remain readable in both namespaces.
        stress = work / "stress"
        stress.mkdir()
        for index in range(160):
            (stress / (f"FILE_{index:03}_" + "X" * 55 + ".BIN")).write_bytes(bytes([index]) * (index + 1))
        actual = work / "stress.iso"
        run_writer(writer, stress, actual, "MULTISECTOR")
        traverse(actual, stress)
        report["reader"].append("160 files with ISO9660/UDF records spanning multiple directory sectors")
        nested = work / "nested"
        for directory in ["A/AA/AAA", "A/AB", "B/BA", "C"]:
            (nested / directory).mkdir(parents=True)
            (nested / directory / "FILE.BIN").write_bytes(directory.encode("ascii"))
        run_writer(writer, nested, actual, "NESTED")
        traverse(actual, nested)
        image = actual.read_bytes()
        pvd = image[16 * SECTOR:17 * SECTOR]
        table_size = int.from_bytes(pvd[132:136], "little")
        table_lba = int.from_bytes(pvd[140:144], "little")
        table = image[table_lba * SECTOR:table_lba * SECTOR + table_size]
        path_records = []
        offset = 0
        while offset < table_size:
            length = table[offset]
            parent = int.from_bytes(table[offset + 6:offset + 8], "little")
            path_records.append((table[offset + 8:offset + 8 + length], parent))
            offset += 8 + length + length % 2
        assert path_records == [(b"\0", 1), (b"A", 1), (b"B", 1), (b"C", 1),
                                (b"AA", 2), (b"AB", 2), (b"BA", 3), (b"AAA", 5)]
        report["reader"].append("nested directories and ISO9660 breadth-first path table parent indices")
        unicode_source = work / "\u4e2d\u6587 source"
        (unicode_source / "\u97f3\u4e50").mkdir(parents=True)
        (unicode_source / "\u97f3\u4e50/\U0001f3b5.bin").write_bytes(b"Unicode payload")
        run_writer(writer, unicode_source, actual, "\u97f3\u4e50")
        inspect_unicode_udf(actual, unicode_source)
        report["reader"].append("Unicode filesystem paths, UDF UTF16 and supplementary characters")
        # Every preflight/publication failure must preserve an existing destination.
        destination = work / "preserved.iso"
        destination.write_bytes(b"existing destination")
        bad = work / "invalid"
        bad.mkdir()
        (bad / ("X" * 220)).write_bytes(b"invalid ISO record")
        for invalid_source in [work / "missing", bad]:
            run_writer(writer, invalid_source, destination, "FAIL", success=False)
            assert destination.read_bytes() == b"existing destination"
        run_writer(writer, source, source / "inside.iso", "FAIL", success=False)
        assert not (source / "inside.iso").exists()
        report["failure"].extend(["missing input preserves destination", "overlong record rejected before publication",
                                  "destination inside source rejected"])
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            kernel = ctypes.WinDLL("kernel32", use_last_error=True)
            create = kernel.CreateFileW
            create.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p,
                               wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
            create.restype = wintypes.HANDLE
            close = kernel.CloseHandle
            close.argtypes = [wintypes.HANDLE]
            close.restype = wintypes.BOOL
            handle = create(str(destination), 0x80000000, 0, None, 3, 0, None)
            assert handle != wintypes.HANDLE(-1).value
            try:
                run_writer(writer, source, destination, "LOCKED", success=False)
                assert not list(work.glob(".dvda-iso-*.tmp"))
            finally:
                close(handle)
            assert destination.read_bytes() == b"existing destination"
            report["failure"].append("locked Windows destination preserves old file and cleans temporary output")
        run_writer(writer, source, destination, "REPLACE")
        traverse(destination, source)
        assert not list(work.glob(".dvda-iso-*.tmp"))
        report["reader"].append("successful atomic replacement of an existing destination")
        boundary = work / "record-boundary"
        boundary.mkdir()
        (boundary / ("Y" * 219)).write_bytes(b"largest supported record")
        run_writer(writer, boundary, destination, "BOUNDARY")
        traverse(destination, boundary)
        report["reader"].append("219-byte filename fits the one-byte ISO9660 record length")
    report['passed'] = True
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Rust ISO acceptance: {len(report['fixtures'])} filesystem fixtures, "
          f"{len(report['reader'])} reader checks, {len(report['failure'])} failure checks: PASS")


if __name__ == "__main__":
    main()
