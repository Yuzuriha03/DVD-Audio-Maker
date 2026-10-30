use crate::bitstream::{Bits, checksum};
use crate::encoder::{pair_matrix, quantization};
use crate::{Config, Error, encode, format};
fn config() -> Config {
    Config {
        sample_rate: 48000,
        bits: 16,
        channels: 2,
        restart_interval: 8,
        metadata: vec![0, 0, 0x40, 0],
    }
}
#[test]
fn silence_has_major_sync_and_padding() {
    let bytes = encode(&config(), &[0; 82], || false).unwrap();
    assert_eq!(&bytes[4..8], &[0xf8, 0x72, 0x6f, 0xbb]);
    assert!(bytes.len() > 32);
}
#[test]
fn restart_interval_matches_native_admission() {
    let mut c = config();
    let expected = encode(&c, &[0; 82], || false).unwrap();
    c.restart_interval = 0;
    assert_eq!(encode(&c, &[0; 82], || false).unwrap(), expected);
    c.restart_interval = 33;
    assert_eq!(encode(&c, &[0; 82], || false), Err(Error::InvalidConfig));
}
#[test]
fn cancellation_discards_output() {
    let mut calls = 0;
    assert_eq!(
        encode(&config(), &[0; 160], || {
            calls += 1;
            calls == 2
        }),
        Err(Error::Cancelled)
    );
}
#[test]
fn rejects_invalid_precision() {
    assert_eq!(encode(&config(), &[1, 0], || false), Err(Error::InvalidPcm));
}
#[test]
fn bit_writer_and_checksum() {
    let mut b = Bits::default();
    b.put(0x1234, 16);
    b.put(0x56, 8);
    assert_eq!(&b.seal(false)[..2], &[0x1234, 0x5600]);
    assert_eq!(checksum(&[]), 0xa9a2);
}
#[test]
fn bit_writer_transactions_preserve_output_and_counting() {
    for count_only in [false, true] {
        for prefix in 0..16 {
            let make = || {
                if count_only {
                    Bits::counting()
                } else {
                    Bits::default()
                }
            };
            let mut writer = make();
            let mut expected = make();
            for target in [&mut writer, &mut expected] {
                target.put(0xabcd, 16);
                target.put((1 << prefix) - 1, prefix);
            }
            let before = (
                writer.words.clone(),
                writer.word_len(),
                writer.pending,
                writer.width,
            );
            let mut discarded = writer.fork();
            discarded.put(u32::MAX, 32);
            drop(discarded);
            assert_eq!(
                (
                    writer.words.clone(),
                    writer.word_len(),
                    writer.pending,
                    writer.width
                ),
                before
            );

            let mut local = writer.fork();
            for (value, width) in [(0x12345678, 32), (5, 3), (0, 0)] {
                local.put(value, width);
                expected.put(value, width);
            }
            writer.commit(local);
            assert_eq!(writer.words, expected.words);
            assert_eq!(writer.word_len(), expected.word_len());
            assert_eq!(
                (writer.pending, writer.width),
                (expected.pending, expected.width)
            );
        }
    }
}

#[test]
fn bulk_writer_matches_bitwise_reference() {
    for prefix in 0..16 {
        for width in 0..=32 {
            for value in [0, u32::MAX, 0x12345678, 0xaaaaaaaa] {
                let value = if width == 32 {
                    value
                } else {
                    value & ((1u32 << width) - 1)
                };
                let mut actual = Bits::default();
                let mut expected = Bits::default();
                let mut counter = Bits::counting();
                for (v, w) in [
                    (0x5555 & ((1 << prefix) - 1), prefix),
                    (value, width),
                    (3, 2),
                ] {
                    actual.put(v, w);
                    counter.put(v, w);
                    for bit in (0..w).rev() {
                        expected.pending = (expected.pending << 1) | ((v >> bit) & 1);
                        expected.width += 1;
                        if expected.width == 16 {
                            expected.words.push(expected.pending);
                            expected.pending = 0;
                            expected.width = 0;
                        }
                    }
                }
                assert_eq!(actual.words, expected.words);
                assert_eq!(
                    (actual.pending, actual.width),
                    (expected.pending, expected.width)
                );
                let mut counted = Bits::default();
                counter.finish_into(&mut counted, true);
                assert_eq!(counted.word_len(), actual.seal(true).len());
            }
        }
    }
}
#[test]
fn bulk_writer_matches_long_bit_sequence() {
    let mut actual = Bits::default();
    let mut expected = Bits::default();
    let mut state = 0x12345678u32;
    for _ in 0..10000 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let width = state % 33;
        let value = if width == 32 {
            state
        } else {
            state & ((1u32 << width) - 1)
        };
        actual.put(value, width);
        for bit in (0..width).rev() {
            expected.pending = (expected.pending << 1) | ((value >> bit) & 1);
            expected.width += 1;
            if expected.width == 16 {
                expected.words.push(expected.pending);
                expected.pending = 0;
                expected.width = 0;
            }
        }
        assert_eq!(
            (actual.pending, actual.width),
            (expected.pending, expected.width)
        );
    }
    assert_eq!(actual.words, expected.words);
    assert_eq!(actual.seal(true), expected.seal(true));
}
#[test]
fn scaling_matrix_and_prediction_compress_correlated_pcm() {
    let c = Config {
        sample_rate: 48000,
        bits: 24,
        channels: 2,
        restart_interval: 8,
        metadata: vec![0, 0, 64, 0],
    };
    let profile = format::Profile::standard(&c);
    let pcm: Vec<_> = (0..4000)
        .flat_map(|n| {
            let x = (n % 1000 - 500) * 256;
            [x, x + 256]
        })
        .collect();
    assert_eq!(pair_matrix(&c, &profile, &pcm), 2);
    let qss = quantization(&c, &profile, &pcm, 2);
    assert_eq!(qss, [8, 8]);
    let bytes = encode(&c, &pcm, || false).unwrap();
    assert!(
        bytes.len() < pcm.len() * 3 / 4,
        "prediction/scaling/matrix must beat raw24bit"
    );
}
