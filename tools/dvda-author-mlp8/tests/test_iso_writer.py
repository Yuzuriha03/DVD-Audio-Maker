import pathlib
import argparse
import shutil
import struct
import subprocess
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[3]
WRITER = ROOT / "tools/dvda-author-mlp8/src/iso_writer.c"
HARNESS = ROOT / "tools/dvda-author-mlp8/tests/iso_writer_harness.c"
SECTOR = 2048


def u16(data, offset):
    return struct.unpack_from("<H", data, offset)[0]


def u32(data, offset):
    return struct.unpack_from("<I", data, offset)[0]


def u64(data, offset):
    return struct.unpack_from("<Q", data, offset)[0]


def udf_crc(data):
    crc = 0
    for value in data:
        crc ^= value << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc


def assert_udf_tag_data(data, expected_id, expected_location=None):
    assert u16(data, 0) == expected_id, (u16(data, 0), expected_id)
    assert sum(data[:4] + data[5:16]) & 0xFF == data[4], "tag checksum"
    crc_length = u16(data, 10)
    assert udf_crc(data[16:16 + crc_length]) == u16(data, 8), "descriptor CRC"
    if expected_location is not None:
        assert u32(data, 12) == expected_location, (lba, u32(data, 12), expected_location)
    return data


def assert_udf_tag(image, lba, expected_id, expected_location=None):
    data = image[lba * SECTOR:(lba + 1) * SECTOR]
    try:
        return assert_udf_tag_data(data, expected_id, expected_location)
    except AssertionError as error:
        raise AssertionError(f"UDF tag at LBA {lba}: {error}") from error


def iso_entries(image, lba, size):
    data = image[lba * SECTOR:lba * SECTOR + size]
    entries = {}
    offset = 0
    while offset < len(data):
        length = data[offset]
        if length == 0:
            offset = ((offset // SECTOR) + 1) * SECTOR
            continue
        record = data[offset:offset + length]
        name_length = record[32]
        name = record[33:33 + name_length].decode("ascii")
        if name not in ("\x00", "\x01"):
            entries[name.removesuffix(";1")] = (u32(record, 2), u32(record, 10))
        offset += length
    return entries


def udf_name(record):
    length = record[19]
    value = record[38:38 + length]
    if not value:
        return ""
    assert value[0] == 8
    return value[1:].decode("ascii")


def udf_entries(image, partition_start, directory_lba, directory_size):
    data = image[directory_lba * SECTOR:directory_lba * SECTOR + directory_size]
    entries = {}
    offset = 0
    while offset < len(data):
        if not any(data[offset:offset + SECTOR - offset % SECTOR]):
            offset = ((offset // SECTOR) + 1) * SECTOR
            continue
        length = (38 + data[offset + 19] + 3) & ~3
        record = data[offset:offset + length]
        assert_udf_tag_data(record, 257,
                            directory_lba - partition_start + offset // SECTOR)
        if record[18] & 8:
            name = ".."
        else:
            name = udf_name(record)
        entries[name] = (partition_start + u32(record, 24), u32(record, 20))
        offset += length
    return entries


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--keep-image", type=pathlib.Path,
                        help="copy the generated ISO to this path for external readers")
    args = parser.parse_args()
    gcc = shutil.which("gcc")
    if not gcc:
        raise SystemExit("gcc is required (MSYS2 MinGW-w64 is supported)")
    expected_order = [
        "AUDIO_PP.IFO", "AUDIO_TS.IFO", "AUDIO_TS.VOB", "AUDIO_TS.BUP",
        "AUDIO_SV.IFO", "AUDIO_SV.VOB", "AUDIO_SV.BUP",
        "ATS_01_0.IFO", "ATS_01_1.AOB", "ATS_01_2.AOB", "ATS_01_0.BUP",
        "ATS_02_0.IFO", "ATS_02_1.AOB", "ATS_02_0.BUP",
    ]
    with tempfile.TemporaryDirectory(prefix="dvda-iso-writer-test-") as temporary:
        work = pathlib.Path(temporary)
        source = work / "source"
        audio_ts = source / "AUDIO_TS"
        audio_ts.mkdir(parents=True)
        contents = {}
        for index, name in enumerate(expected_order):
            payload = bytes([index + 1]) * (SECTOR + 31 + index)
            (audio_ts / name).write_bytes(payload)
            contents[name] = payload
        harness = work / "iso_writer_test.exe"
        subprocess.run([gcc, "-std=gnu11", "-Wall", "-Wextra", "-Werror",
                        str(WRITER), str(HARNESS), "-o", str(harness)], check=True)
        output = work / "sample.iso"
        subprocess.run([str(harness), str(source), str(output), "DVD-AUDIO-TEST"], check=True)
        if args.keep_image:
            args.keep_image.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(output, args.keep_image)
        image = output.read_bytes()

        pvd = image[16 * SECTOR:17 * SECTOR]
        assert pvd[:7] == b"\x01CD001\x01"
        assert u32(pvd, 80) == len(image) // SECTOR
        assert pvd[181] == 2
        assert u16(pvd, 184) == 1 and struct.unpack_from(">H", pvd, 186)[0] == 1
        assert pvd[881] == 1
        root_lba = u32(pvd, 158)
        root_size = u32(pvd, 166)
        root = iso_entries(image, root_lba, root_size)
        audio_lba, audio_size = root["AUDIO_TS"]
        audio = iso_entries(image, audio_lba, audio_size)
        assert list(audio) == sorted(expected_order), "ISO directory records remain alphabetic"
        assert list(sorted(audio, key=lambda name: audio[name][0])) == expected_order
        for name, payload in contents.items():
            lba, size = audio[name]
            assert size == len(payload)
            assert image[lba * SECTOR:lba * SECTOR + size] == payload

        assert image[18 * SECTOR + 1:18 * SECTOR + 6] == b"BEA01"
        assert image[19 * SECTOR + 1:19 * SECTOR + 6] == b"NSR02"
        assert image[20 * SECTOR + 1:20 * SECTOR + 6] == b"TEA01"
        anchor = assert_udf_tag(image, 256, 2, 256)
        assert (u32(anchor, 16), u32(anchor, 20)) == (16 * SECTOR, 32)
        assert (u32(anchor, 24), u32(anchor, 28)) == (16 * SECTOR, 48)
        descriptor_ids = [assert_udf_tag(image, 32 + i, expected, 32 + i)
                          for i, expected in enumerate([1, 4, 5, 6, 7, 8])]
        assert [u16(value, 0) for value in descriptor_ids] == [1, 4, 5, 6, 7, 8]
        assert u32(descriptor_ids[2], 188) == 257
        assert u32(descriptor_ids[3], 436) == 64
        reserve_descriptors = [assert_udf_tag(image, 48 + i, expected, 48 + i)
                               for i, expected in enumerate([1, 4, 5, 6, 7, 8])]
        assert [u16(value, 0) for value in reserve_descriptors] == [1, 4, 5, 6, 7, 8]
        integrity = assert_udf_tag(image, 64, 9, 64)
        assert_udf_tag(image, 65, 8, 65)
        end_anchor_lba = len(image) // SECTOR - 1
        end_anchor = assert_udf_tag(image, end_anchor_lba, 2, end_anchor_lba)
        assert (u32(end_anchor, 20), u32(end_anchor, 28)) == (32, 48)
        assert u32(descriptor_ids[2], 192) == end_anchor_lba - 257

        fsd = assert_udf_tag(image, 257, 256, 0)
        assert_udf_tag(image, 258, 8, 1)
        partition_start = u32(descriptor_ids[2], 188)
        root_fe_lba = partition_start + u32(fsd, 404)
        root_fe = assert_udf_tag(image, root_fe_lba, 261, root_fe_lba - partition_start)
        root_dir_lba = partition_start + u32(root_fe, 180)
        root_dir_size = u32(root_fe, 176)
        root_udf = udf_entries(image, partition_start, root_dir_lba, root_dir_size)
        audio_fe_lba, _ = root_udf["AUDIO_TS"]
        audio_fe = assert_udf_tag(image, audio_fe_lba, 261, audio_fe_lba - partition_start)
        audio_dir_lba = partition_start + u32(audio_fe, 180)
        audio_dir_size = u32(audio_fe, 176)
        audio_udf = udf_entries(image, partition_start, audio_dir_lba, audio_dir_size)
        for name, payload in contents.items():
            file_fe_lba, _ = audio_udf[name]
            file_fe = assert_udf_tag(image, file_fe_lba, 261,
                                     file_fe_lba - partition_start)
            assert u64(file_fe, 56) == len(payload)
            file_extent_lba = partition_start + u32(file_fe, 180)
            assert file_extent_lba == audio[name][0]
            assert image[file_extent_lba * SECTOR:file_extent_lba * SECTOR + len(payload)] == payload
        assert u32(integrity, 120) == len(contents)
        assert u32(integrity, 124) == 2

        try:
            import pycdlib
        except ImportError:
            pycdlib = None
        if pycdlib is not None:
            mounted = pycdlib.PyCdlib()
            mounted.open(str(output))
            iso_tree = list(mounted.walk(iso_path="/"))
            iso_audio = next(files for path, dirs, files in iso_tree
                             if path.rstrip("/") == "/AUDIO_TS")
            assert sorted(name.split(";", 1)[0] for name in iso_audio) == sorted(expected_order)
            udf_tree = list(mounted.walk(udf_path="/"))
            udf_audio = next(files for path, dirs, files in udf_tree
                             if path.rstrip("/") == "/AUDIO_TS")
            assert sorted(udf_audio) == sorted(expected_order)
            extracted = work / "extracted.bin"
            mounted.get_file_from_iso(str(extracted),
                                      udf_path="/AUDIO_TS/AUDIO_TS.VOB")
            assert extracted.read_bytes() == contents["AUDIO_TS.VOB"]
            mounted.close()
            print("pycdlib ISO9660/UDF traversal and UDF file extraction: PASS")

    print("ISO9660 DVD-Audio ordering, PVD fields, UDF descriptors, and shared file extents: PASS")


if __name__ == "__main__":
    main()
