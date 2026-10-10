//! Compare all ATSI bytes with a frozen C output.
//! Usage: atsi-parity ATS_01_0.IFO [tracks.csv]
//! CSV rows: title,mlp,channels,bits,rate,cga,first_pts,pts_length,first_sector,last_sector,downmix_rank
//! Without CSV, compares the original 96 kHz stereo MLP oracle fixture.

use dvda_author::{
    atsi::{self, Options, Title, Track},
    samg,
};
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let path = args.next().ok_or("Expected oracle ATS_01_0.IFO path")?;
    let metadata = match args.next() {
        Some(path) => fs::read_to_string(path)?,
        None => "1,1,2,16,96000,1,98,11325,0,16,0\n".to_string(),
    };
    let mut titles: Vec<Title> = Vec::new();
    for line in metadata
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
    {
        let values: Vec<u32> = line
            .split(',')
            .map(|v| v.trim().parse())
            .collect::<Result<_, _>>()?;
        if values.len() != 11 {
            return Err("Expected 11 CSV fields per track".into());
        }
        let title = usize::try_from(values[0])?;
        if title == titles.len() + 1 {
            titles.push(Title { tracks: Vec::new() });
        }
        if title == 0 || title != titles.len() || values[1] > 1 {
            return Err("Expected ascending contiguous title numbers and mlp 0/1".into());
        }
        titles[title - 1].tracks.push(Track {
            audio: samg::Track {
                mlp: values[1] == 1,
                channels: u8::try_from(values[2])?,
                bits: u8::try_from(values[3])?,
                rate: values[4],
                channel_assignment: u8::try_from(values[5])?,
                first_pts: values[6],
                pts_length: values[7],
                first_sector: values[8],
                last_sector: values[9],
            },
            downmix_table_rank: u8::try_from(values[10])?,
            pictures: Vec::new(),
        });
    }
    let actual = atsi::encode(&titles, &Options::default())?;
    let expected = fs::read(path)?;
    if actual.bytes != expected {
        let offset = actual.bytes.iter().zip(&expected).position(|(a, b)| a != b);
        return Err(format!(
            "ATSI mismatch at {offset:?}; sizes {} / {}",
            actual.bytes.len(),
            expected.len()
        )
        .into());
    }
    println!(
        "ATSI whole-file parity: {} bytes; {} sectors; {} titles",
        actual.bytes.len(),
        actual.sectors,
        titles.len()
    );
    Ok(())
}
