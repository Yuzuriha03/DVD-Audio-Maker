use dvda_author::samg::{Layout, Track, encode};
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("Expected oracle AUDIO_PP.IFO path")?;
    let track = Track {
        mlp: true,
        channels: 2,
        bits: 16,
        rate: 96000,
        channel_assignment: 1,
        first_pts: 98,
        pts_length: 11325,
        first_sector: 0,
        last_sector: 16,
    };
    let layout = Layout {
        start_sector: 278,
        samg_sectors: 64,
        amg_sectors: 3,
        asvs_sectors: 0,
        atsi_sectors: vec![2],
        still_vob_sectors: 0,
        top_vob_sectors: 0,
        video_link_tracks: 0,
    };
    let (actual, last) = encode(&[vec![track]], &layout)?;
    let expected = fs::read(path)?;
    if actual != expected {
        let offset = actual.iter().zip(&expected).position(|(a, b)| a != b);
        return Err(format!(
            "SAMG mismatch at {offset:?}; sizes {} / {}",
            actual.len(),
            expected.len()
        )
        .into());
    }
    println!(
        "SAMG real stereo MLP parity: {} bytes; last sector {last}",
        actual.len()
    );
    Ok(())
}
