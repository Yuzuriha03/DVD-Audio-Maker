//! Read-only check of an authored menu's navigation and overlay XML.
use std::{error::Error, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("Usage: menu-buttons-check DIRECTORY TOTAL_PAGES INDEX_PAGES".into());
    }
    let total: usize = args[1].parse()?;
    let index: usize = args[2].parse()?;
    let issues = dvda_core::menu_check::buttons(Path::new(&args[0]), total, index);
    if !issues.is_empty() {
        eprintln!("{}", serde_json::to_string_pretty(&issues)?);
        std::process::exit(1);
    }
    println!(
        "PASS: {total} menu pages and their overlay/navigation buttons match ({index} index pages)"
    );
    Ok(())
}
