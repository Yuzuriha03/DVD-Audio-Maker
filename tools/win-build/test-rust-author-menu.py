"""Exercise the complete Rust author with real image, MPEG and menu backends."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import wave
import zlib

import pycdlib

ROOT = Path(__file__).resolve().parents[2]
SECTOR = 2048


def require(ok, message):
    if not ok:
        raise AssertionError(message)


def be16(data, offset):
    return struct.unpack_from(">H", data, offset)[0]


def be32(data, offset):
    return struct.unpack_from(">I", data, offset)[0]


def png(path, rank):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = []
    for y in range(240):
        row = bytearray([0])
        for x in range(320):
            row.extend(((x + rank * 47) % 192 + 20, (y + rank * 19) % 128 + 20,
                        ((x // 20 + y // 15) % 2) * 80 + 40))
        rows.append(row)
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 320, 240, 8, 2, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b""))


def wav(path, rank):
    frames = b"".join(struct.pack("<hh", (i * (rank + 3) % 30000) - 15000,
                                   15000 - i * (rank + 7) % 30000) for i in range(4800))
    with wave.open(str(path), "wb") as output:
        output.setnchannels(2)
        output.setsampwidth(2)
        output.setframerate(48000)
        output.writeframes(frames)
    return b"".join(frames[i:i+2][::-1] for i in range(0, len(frames), 2))


def pcm_payload(aob):
    result = bytearray()
    for sector in (aob[i:i+SECTOR] for i in range(0, len(aob), SECTOR)):
        pes = sector.find(b"\x00\x00\x01\xbd")
        require(pes >= 0, "AOB sector has no audio PES")
        end = pes + 6 + be16(sector, pes + 4)
        private = pes + 9 + sector[pes + 8]
        require(sector[private] == 0xa0, "Fixture expected LPCM substream")
        start = private + 4 + sector[private + 3]
        require(start <= end <= SECTOR, "Audio packet boundary")
        result.extend(sector[start:end])
    return bytes(result)


def filesystem(image, disc):
    mounted = pycdlib.PyCdlib()
    mounted.open(str(image))
    expected = {p.relative_to(disc).as_posix(): p.read_bytes() for p in disc.rglob("*") if p.is_file()}
    lbas = {}
    try:
        for namespace in ("iso", "udf"):
            actual = {}
            for directory, _, files in mounted.walk(**{namespace + "_path": "/"}):
                for name in files:
                    path = directory.rstrip("/") + "/" + name
                    output = io.BytesIO()
                    mounted.get_file_from_iso_fp(output, **{namespace + "_path": path})
                    key = path.lstrip("/").removesuffix(";1")
                    actual[key] = output.getvalue()
                    if namespace == "iso":
                        lbas[key] = mounted.get_record(iso_path=path).extent_location()
            require(actual == expected, namespace + " extraction differs from authored tree")
    finally:
        mounted.close()
    return lbas


def manager(disc, lbas, menus, still_counts, norm, expected_pcm):
    directory = disc / "AUDIO_TS"
    amg = (directory / "AUDIO_TS.IFO").read_bytes()
    require(amg == (directory / "AUDIO_TS.BUP").read_bytes(), "AMG backup")
    amg_lba = lbas["AUDIO_TS/AUDIO_TS.IFO"]
    require(amg_lba + be32(amg, 0xc0) == lbas["AUDIO_TS/AUDIO_TS.VOB"], "AMGM physical pointer")
    require(amg_lba + be32(amg, 12) + 1 - len(amg) // SECTOR == lbas["AUDIO_TS/AUDIO_TS.BUP"], "AMG backup physical pointer")
    expected_attribute = 0x43 if norm == "ntsc" else 0x53
    require(amg[0x100] == expected_attribute, "AMG actual norm")
    base = be32(amg, 0xcc) * SECTOR
    pages = be16(amg, base + 16)
    require(pages == menus, "AMG page count")
    ranges = []
    for page in range(pages):
        pgc = base + 16 + be32(amg, base + 28 + page * 8)
        first, copy, last = (be32(amg, pgc + o) for o in (286, 294, 298))
        require(first == copy and (first == 0 if page == 0 else first == ranges[-1][1] + 1), "PGCI cell chain")
        require(last >= first, "PGCI reversed cell")
        require(be16(amg, pgc + 156) == (page + 2 if page + 1 < pages else 0), "PGCI next page")
        require(be16(amg, pgc + 158) == (page if page else 0), "PGCI previous page")
        ranges.append((first, last))
    require(ranges[-1][1] + 1 == (directory / "AUDIO_TS.VOB").stat().st_size // SECTOR, "PGCI exact VOB end")
    samg = (directory / "AUDIO_PP.IFO").read_bytes()
    matrix = len(samg) // 8
    require(all(samg[i*matrix:(i+1)*matrix] == samg[:matrix] for i in range(8)), "SAMG eight copies")
    track = 0
    picture_title = 0
    for group, payload in enumerate(expected_pcm, 1):
        atsi = (directory / f"ATS_{group:02}_0.IFO").read_bytes()
        require(atsi == (directory / f"ATS_{group:02}_0.BUP").read_bytes(), "ATSI backup")
        require(atsi[:12] == b"DVDAUDIO-ATS", "ATSI signature")
        aob = (directory / f"ATS_{group:02}_1.AOB").read_bytes()
        require(pcm_payload(aob) == b"".join(payload), "LPCM payload is not lossless")
        aob_lba = lbas[f"AUDIO_TS/ATS_{group:02}_1.AOB"]
        # This fixture keeps consecutive tracks in one title.
        title = 0x800 + be32(atsi, 0x80c)
        require(be16(atsi, 0x800) == 1, "Fixture title boundary")
        group_pictures = still_counts[track:track + len(payload)]
        if sum(group_pictures):
            picture_title += 1
        picture_table = title + be16(atsi, title + 14)
        preceding = 0
        for index in range(len(payload)):
            record = 16 + 52 * track
            require(samg[record+2:record+4] == bytes([group, index + 1]), "SAMG group/track sequence")
            first = be32(samg, record + 40)
            last = be32(samg, record + 48)
            require(first == be32(samg, record + 44) and aob_lba <= first <= last < aob_lba + len(aob) // SECTOR,
                    "SAMG physical track extent")
            sector_record = title + be16(atsi, title + 12) + 12 * index
            require(first == aob_lba + be32(atsi, sector_record + 4)
                    and last == aob_lba + be32(atsi, sector_record + 8), "ATSI and SAMG exact track extent")
            row = picture_table + 6 * index
            require(atsi[row] == picture_title, "ATSI ASVS picture-title reference")
            begin = len(payload) * 6 + preceding * 10
            count = group_pictures[index]
            require(be16(atsi, row + 2) == begin and be16(atsi, row + 4) == begin + 10 * count - 1,
                    "ATSI picture range / previous-picture reuse")
            for rank in range(count):
                picture = picture_table + begin + 10 * rank
                require(atsi[picture] == preceding + rank + 1 and atsi[picture + 3] == index + 1,
                        "ATSI picture and track sequence")
            preceding += count
            track += 1
        for table in (0x800, be32(amg, 0xc8) * SECTOR):
            for index in range(be16(amg, table)):
                record = table + 4 + 14 * index
                if amg[record+8] == group:
                    require(amg_lba + be32(amg, record + 10) == lbas[f"AUDIO_TS/ATS_{group:02}_0.IFO"], "AMG ATSI physical pointer")
    require(be16(samg, 12) == track, "SAMG total tracks")
    asvs = (directory / "AUDIO_SV.IFO").read_bytes()
    require(asvs == (directory / "AUDIO_SV.BUP").read_bytes(), "ASVS backup")
    require(amg_lba + be32(amg, 0x30) == lbas["AUDIO_TS/AUDIO_SV.IFO"], "AMG ASVS physical pointer")
    require(asvs[0x18] == expected_attribute, "ASVS actual norm")
    require(be32(asvs, 0x14) + 1 == (directory / "AUDIO_SV.VOB").stat().st_size // SECTOR, "ASVS exact VOB end")
    total = 0
    for title in range(be16(asvs, 12)):
        record = 0x60 + 8 * title
        require(be16(asvs, record + 2) == total + 1, "ASVS picture sequence")
        require(be32(asvs, record + 4) <= be32(asvs, 0x14), "ASVS picture title extent")
        total += asvs[record]
    require(total == sum(still_counts), "ASVS all still pictures")
    video = (directory / "AUDIO_SV.VOB").read_bytes()
    ends = [i + SECTOR for i in range(0, len(video), SECTOR) if video[i:i+4] == b"\0\0\1\xb9"]
    require(len(ends) == total and ends[-1] == len(video), "Every still has a separate end sector")
    begin = 0
    for end in ends:
        segment = video[begin:end]
        require(segment[0x400:0x404] == b"\xff" * 4, "Still DSI removed")
        extension = segment.find(b"\0\0\1\xb5")
        require(extension >= 0 and segment[extension+4] >> 4 == 1
                and segment[extension+5] & 8, "Still progressive sequence flag")
        begin = end
    return {"pages": pages, "pgci_ranges": ranges, "stills": total, "tracks": track}


def run_case(runtime, output, norm, albums, verifier):
    case = output / f"{norm}-{albums}albums"
    case.mkdir()
    fixtures = case / "fixtures"
    fixtures.mkdir()
    pictures, tracks, pcm = [], [], []
    for index in range(albums):
        picture = fixtures / f"cover {index}.png"
        png(picture, index)
        pictures.append(picture)
        track = fixtures / f"track {index}.wav"
        pcm.append(wav(track, index))
        tracks.append(track)
    index_pages = (albums + 11) // 12
    labels = ["かな 日本", "한국 앨범"] + [f"Album {n}" for n in range(3, albums + 1)]
    labels = labels[:albums]
    index_text = [f"INDEX{i+1}=" + ",".join(labels[i*12:(i+1)*12]) for i in range(index_pages)]
    text = "DVD=" + ":".join(index_text + [f"{label}=Track {i+1}" for i, label in enumerate(labels)])
    covers = fixtures / "covers.txt"
    covers.write_text("\n".join(map(str, pictures)), encoding="utf-8")
    # A multi-picture first track and a reused second track exercise the
    # independent ASVS picture count and ATSI per-track metadata.
    stills = [str(p) for p in pictures]
    if albums >= 2:
        stills[0] = str(pictures[0]) + "," + str(pictures[1])
        stills[1] = ""
    split = max(1, albums // 2)
    groups = [tracks[:split], tracks[split:]] if split < albums else [tracks]
    expected_pcm = [pcm[:split], pcm[split:]] if split < albums else [pcm]
    disc, temporary, image = case / "disc", case / "temporary", case / "disc.iso"
    arguments = [str(runtime / "dvda-author-dev.exe")]
    for group in groups:
        arguments += ["-g"] + list(map(str, group))
    arguments += ["-o", str(disc), "-D", str(temporary), "--topmenu", "--norm", norm,
                  "--nmenus", str(albums + index_pages), "--index-pages", str(index_pages),
                  "--screentext", text, "--index-covers", str(covers), "--background", ",".join(map(str, pictures)),
                  "--fontname", str(Path(os.environ["WINDIR"]) / "Fonts/arial.ttf"),
                  "--fontname-jp", str(Path(os.environ["WINDIR"]) / "Fonts/msgothic.ttc"),
                  "--fontname-kr", str(Path(os.environ["WINDIR"]) / "Fonts/malgun.ttf"),
                  "--stillpics", ";".join(stills), "--iso=" + str(image), "--iso-volume", "RUST-MENU"]
    result = subprocess.run(arguments, cwd=ROOT, capture_output=True)
    (case / "author.log").write_bytes(result.stdout + result.stderr)
    require(result.returncode == 0, "Author failed: " + str(case / "author.log"))
    require(b"UnableToReadFont" not in result.stdout + result.stderr, "Japanese/Korean/default font resolution")
    lbas = filesystem(image, disc)
    counts = [len(entry.split(",")) if entry else 0 for entry in stills]
    checks = manager(disc, lbas, albums + index_pages, counts, norm, expected_pcm)
    project = (temporary / "xmltemp").read_text(encoding="utf-8")
    require(project.count("<pgc>") == checks["pages"], "Actual navigation XML pages")
    for page in range(checks["pages"]):
        document = (temporary / f"spu_xmltemp_{page}.xml").read_text(encoding="utf-8")
        require('format="' + norm.upper() + '"' in document, "SPU actual norm")
        require((temporary / f"impic{page}.png").stat().st_size > 100, "Normal text overlay")
        require((temporary / f"hlpic{page}.png").stat().st_size > 100, "Highlight overlay")
    if verifier:
        index = case / "mlp_index.json"
        index_data = {str(track): {"src": str(track), "mlp_source": "lpcm"} for track in tracks}
        index_data["__discs__"] = [{"iso": image.name,
                                  "groups": [{"group": rank + 1, "tracks": [{"mlp": str(track), "src": str(track)} for track in group]}
                                             for rank, group in enumerate(groups)],
                                  "menu": {"pages": albums + index_pages, "index_pages": index_pages,
                                           "albums": albums, "stills": sum(counts)}}]
        index.write_text(json.dumps(index_data), encoding="utf-8")
        profile = case / "verify-profile.json"
        profile.write_text(json.dumps({"Version": 1, "Language": "en", "Values": {
            "DVDA_MENU": "on", "DVDA_BUILD_DIR": str(case), "DVDA_FINAL_DIR": str(case)}}), encoding="utf-8")
        verification = subprocess.run([str(verifier), "verify", "menu", "--profile", str(profile),
                                       "--iso", str(image)], capture_output=True)
        (case / "menu-verify.log").write_bytes(verification.stdout + verification.stderr)
        require(verification.returncode == 0, "Rust menu_verify failed: " + str(case / "menu-verify.log"))
        require(b'"TrackCount": ' + str(albums).encode() in verification.stdout,
                "Rust verifier did not load the expected menu plan")
        checks["rust_menu_verify"] = True
    checks.update({"case": case.name, "iso_bytes": image.stat().st_size,
                   "iso_sha256": hashlib.sha256(image.read_bytes()).hexdigest(), "iso_udf_payloads_match": True})
    print("PASS " + case.name, flush=True)
    return checks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--author", type=Path, default=ROOT / "rust/target/x86_64-pc-windows-gnu/debug/dvda-author-dev.exe")
    parser.add_argument("--dll-directory", type=Path, default=ROOT / "build/rust-author-iso-fullbridge")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--verifier", type=Path)
    parser.add_argument("--albums", type=int, default=2)
    parser.add_argument("--norm", choices=("pal", "ntsc", "both"), default="both")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    runtime = output / "runtime"
    runtime.mkdir()
    shutil.copy2(args.author, runtime / "dvda-author-dev.exe")
    for source in args.dll_directory.glob("*.dll"):
        shutil.copy2(source, runtime / source.name)
    for name in ("policy.xml", "colors.xml"):
        shutil.copy2(ROOT / "build/rust-image-runtime" / name, runtime / name)
    verifier = None
    if args.verifier:
        verifier = runtime / "dvda-cli.exe"
        shutil.copy2(args.verifier, verifier)
    cases = [run_case(runtime, output, norm, args.albums, verifier)
             for norm in (("pal", "ntsc") if args.norm == "both" else (args.norm,))]
    (output / "report.json").write_text(json.dumps({"status": "PASS", "author_sha256": hashlib.sha256(args.author.read_bytes()).hexdigest(),
                                                     "cases": cases}, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
