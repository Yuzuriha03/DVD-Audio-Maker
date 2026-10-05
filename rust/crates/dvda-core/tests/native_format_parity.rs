//! ISO fixture preserved from the historical C# IsoMigrationTests.
use dvda_core::formats;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn record(name: &[u8], lba: u32, size: u32, directory: bool, attributes: u8) -> Vec<u8> {
    let mut data = vec![0; 33 + name.len() + usize::from(name.len().is_multiple_of(2))];
    data[0] = data.len() as u8;
    data[1] = attributes;
    data[2..6].copy_from_slice(&lba.to_le_bytes());
    data[6..10].copy_from_slice(&lba.to_be_bytes());
    data[10..14].copy_from_slice(&size.to_le_bytes());
    data[14..18].copy_from_slice(&size.to_be_bytes());
    data[25] = if directory { 2 } else { 0 };
    data[28] = 1;
    data[31] = 1;
    data[32] = name.len() as u8;
    data[33..33 + name.len()].copy_from_slice(name);
    data
}

#[test]
fn iso_public_entrypoints_match_historical_extended_attribute_and_truncation_fixture() {
    let root =
        Scratch(std::env::temp_dir().join(format!("dvda-iso-parity-{}", std::process::id())));
    fs::create_dir_all(&root.0).unwrap();
    let path = root.0.join("中文-日本語.iso");
    let mut bytes = vec![0; 24 * 2048];
    let pvd = &mut bytes[16 * 2048..17 * 2048];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    pvd[40..72].fill(32);
    pvd[40..56].copy_from_slice(b"RUST ISO FIXTURE");
    pvd[128..130].copy_from_slice(&2048u16.to_le_bytes());
    pvd[130..132].copy_from_slice(&2048u16.to_be_bytes());
    pvd[156..190].copy_from_slice(&record(&[0], 20, 2048, true, 0));
    let root_records = [
        record(&[0], 20, 2048, true, 0),
        record(&[1], 20, 2048, true, 0),
        record(b"AUDIO_TS", 21, 2048, true, 0),
    ]
    .concat();
    bytes[20 * 2048..20 * 2048 + root_records.len()].copy_from_slice(&root_records);
    let audio_records = [
        record(&[0], 21, 2048, true, 0),
        record(&[1], 20, 2048, true, 0),
        record(b"ATS_01_0.IFO;1", 22, 5, false, 1),
        record(b"EMPTY.BIN;1", 24, 0, false, 0),
    ]
    .concat();
    bytes[21 * 2048..21 * 2048 + audio_records.len()].copy_from_slice(&audio_records);
    let payload = [0, 1, 128, 254, 255];
    bytes[23 * 2048..23 * 2048 + 5].copy_from_slice(&payload);
    fs::write(&path, &bytes).unwrap();
    let request = |inner: &str| json!({"Path":path,"InnerPath":inner});
    assert_eq!(
        formats::dispatch("iso.info", json!(path)).unwrap()["VolumeIdentifier"],
        "RUST ISO FIXTURE"
    );
    assert_eq!(
        formats::dispatch("iso.list", request("")).unwrap()[0]["Name"],
        "AUDIO_TS"
    );
    assert_eq!(
        formats::dispatch("iso.list", request("AUDIO_TS"))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        formats::dispatch("iso.entry", request("missing")).unwrap(),
        Value::Null
    );
    assert_eq!(
        formats::dispatch("iso.list", request("missing")).unwrap(),
        json!([])
    );
    assert_eq!(
        formats::dispatch("iso.data_lba", request("audio_ts/ats_01_0.ifo")).unwrap(),
        23
    );
    assert_eq!(
        formats::dispatch("iso.all_paths", request("")).unwrap(),
        json!(["/AUDIO_TS", "/AUDIO_TS/ATS_01_0.IFO", "/AUDIO_TS/EMPTY.BIN"])
    );
    assert_eq!(
        formats::dispatch("iso.read_file", request("AUDIO_TS/ATS_01_0.IFO")).unwrap()["DataHex"],
        "000180feff"
    );
    assert_eq!(
        formats::dispatch("iso.read_file", request("AUDIO_TS/EMPTY.BIN")).unwrap()["DataHex"],
        ""
    );
    let destination = root.0.join("extracted");
    assert_eq!(
        formats::dispatch(
            "iso.extract",
            json!({"Path":path,"InnerPath":"AUDIO_TS","Destination":destination})
        )
        .unwrap()["Extracted"],
        true
    );
    assert_eq!(fs::read(destination.join("ATS_01_0.IFO")).unwrap(), payload);
    assert_eq!(
        fs::metadata(destination.join("EMPTY.BIN")).unwrap().len(),
        0
    );
    assert_eq!(
        formats::dispatch(
            "iso.extract",
            json!({"Path":path,"InnerPath":"missing","Destination":root.0.join("absent")})
        )
        .unwrap()["Extracted"],
        false
    );
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len((23 * 2048 + 3) as u64)
        .unwrap();
    assert_eq!(
        formats::dispatch("iso.read_file", request("AUDIO_TS/ATS_01_0.IFO")).unwrap()["DataHex"],
        "000180"
    );
    let partial = root.0.join("partial.bin");
    assert_eq!(
        formats::dispatch(
            "iso.extract",
            json!({"Path":path,"InnerPath":"AUDIO_TS/ATS_01_0.IFO","Destination":partial})
        )
        .unwrap()["Extracted"],
        true
    );
    assert_eq!(fs::read(partial).unwrap(), payload[..3]);
    // The verification streaming API intentionally rejects an incomplete ISO
    // payload while developer legacy reads preserve the old available-prefix rule.
    assert!(
        formats::iso_file_chunks(&path, "AUDIO_TS/ATS_01_0.IFO", 2048)
            .unwrap()
            .next_chunk()
            .is_err()
    );
    for (input, expected) in [
        ("ATS_01_0.IFO;1", "ATS_01_0.IFO"),
        ("ATSI;12", "ATSI"),
        ("NAME;X", "NAME;X"),
        ("NAME;", "NAME;"),
        ("A;1;2", "A;1"),
    ] {
        assert_eq!(
            formats::dispatch("iso.strip_version", json!(input)).unwrap(),
            expected
        );
    }
}

#[test]
fn alac_end_marker_extra_bits_and_cookie_follow_historical_parser_contract() {
    use dvda_core::preparation::alac::{self, Cookie, Packet};
    let cookie = Cookie {
        max_samples_per_frame: 1,
        sample_size: 16,
        channels: 2,
        sample_rate: 48000,
        max_coded_frame_size: 0,
        offset: 0,
    };
    for flags in [0x02, 0x06] {
        let mut data = [0; 8];
        data[2] = flags;
        let packets = [Packet {
            position: 0,
            size: 8,
            presentation_time: 32.0,
        }];
        let patches = alac::find_patches(&data, &cookie, &packets).unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].end_bit, 55);
        assert_eq!(patches[0].previous_bits, 0);
        alac::apply_bits(&mut data, patches.iter().map(|p| (p.position, p.end_bit))).unwrap();
        assert_eq!(data[6] & 1, 1);
        assert_eq!(data[7] & 0xc0, 0xc0);
        assert!(
            alac::find_patches(&data, &cookie, &packets)
                .unwrap()
                .is_empty()
        );
    }
    let mut data = [0; 24];
    data[..4].copy_from_slice(&4096u32.to_be_bytes());
    data[5] = 24;
    data[9] = 2;
    data[20..24].copy_from_slice(&96000u32.to_be_bytes());
    let parsed = alac::parse_cookie(&data, 17).unwrap();
    assert_eq!(
        (
            parsed.max_samples_per_frame,
            parsed.sample_size,
            parsed.channels,
            parsed.sample_rate,
            parsed.offset
        ),
        (4096, 24, 2, 96000, 17)
    );
    for length in 0..24 {
        assert!(alac::parse_cookie(&data[..length], 0).is_none());
    }
    data[5] = 17;
    assert!(alac::parse_cookie(&data, 0).is_none());
}
