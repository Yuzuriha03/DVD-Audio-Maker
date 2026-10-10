use std::{env, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if !(2..=3).contains(&args.len()) {
        return Err("Usage: iso-write SOURCE DESTINATION [VOLUME]".into());
    }
    let label = args
        .get(2)
        .map(|value| value.to_str().ok_or("Volume label must be Unicode"))
        .transpose()?;
    dvda_author::iso::write(Path::new(&args[0]), Path::new(&args[1]), label)?;
    Ok(())
}
