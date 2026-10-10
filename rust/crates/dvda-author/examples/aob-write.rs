//! Isolated packetizer acceptance entry point; production uses author_group.
use dvda_author::{
    Silent,
    aob::{self, TrackInput},
};
use std::path::PathBuf;
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    let output = PathBuf::from(
        arguments
            .next()
            .ok_or("Usage: aob-write OUTPUT [--new-title] TRACK ...")?,
    );
    let mut tracks = vec![];
    let mut new_title = false;
    let mut cga = None;
    let mut downmix = 0;
    let mut split = 524288;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--new-title") => new_title = true,
            Some("--cga") => {
                cga = Some(
                    arguments
                        .next()
                        .and_then(|s| s.to_str().and_then(|s| s.parse::<u8>().ok()))
                        .ok_or("Invalid CGA")?,
                )
            }
            Some("--downmix") => {
                downmix = arguments
                    .next()
                    .and_then(|s| s.to_str().and_then(|s| s.parse::<u8>().ok()))
                    .ok_or("Invalid downmix")?
            }
            Some("--split-sectors") => {
                split = arguments
                    .next()
                    .and_then(|s| s.to_str().and_then(|s| s.parse::<u32>().ok()))
                    .ok_or("Invalid split")?
            }
            _ => {
                tracks.push(TrackInput {
                    path: argument.into(),
                    new_title,
                    cga,
                    downmix_rank: downmix,
                });
                new_title = false;
            }
        }
    }
    let result = aob::author_group_with_split(&output, 1, &tracks, &mut Silent, split)?;
    for (i, track) in result.tracks.iter().enumerate() {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            i,
            track.first_sector,
            track.last_sector,
            track.first_pts,
            track.pts_length,
            track.mlp as u8,
            track.bits,
            track.rate,
            track.channels,
            track.channel_assignment
        );
    }
    println!("sectors\t{}", result.sectors);
    println!("titles\t{:?}", result.title_starts);
    Ok(())
}
