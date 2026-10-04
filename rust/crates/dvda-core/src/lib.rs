pub mod aob;
pub mod author;
pub mod config;
pub mod formats;
pub mod hash;
pub mod identity;
pub mod lpcm;
pub mod menu;
pub mod options;
pub mod planner;
pub mod probes;
pub mod publication;
pub mod validation;
pub mod workflow;

use serde_json::{Value, json};

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "iso.info"
        | "iso.list"
        | "iso.entry"
        | "iso.all_paths"
        | "iso.data_lba"
        | "iso.read_file"
        | "iso.extract"
        | "iso.strip_version"
        | "wav.layout"
        | "wav.normalize"
        | "flac.comments"
        | "flac.picture"
        | "flac.replace_picture"
        | "alac.cookie"
        | "alac.find_bad_frames"
        | "alac.apply_patches" => formats::dispatch(operation, request),
        "config.parse" => Ok(json!(config::parse(
            request.as_str().ok_or("Expected config text")?
        ))),
        "disc.plan" => planner::plan(request),
        "options.defaults" | "options.evaluate" | "shell.assignment" => {
            options::dispatch(operation, request)
        }
        "tracks.sort" => planner::sort(request),
        "track.number" => planner::track_number_value(request),
        "math.estimate_aob" => planner::estimate_value(request),
        "math.content_limit" => planner::content_limit_value(request),
        "menu.sanitize" => menu::sanitize(request),
        "menu.truncate" => menu::truncate(request),
        "menu.font_size" => menu::font_size(request),
        "menu.font_width" => menu::font_width(request),
        "menu.short_album" => menu::short_album(request),
        "menu.normalize_path" => menu::normalize_path(request),
        "menu.pages" => menu::pages(request),
        "menu.visual_near_solid" => menu::visual_near_solid(request),
        "menu.visual_background_invalid" => menu::visual_background_invalid(request),
        "menu.visual_thumbnail_missing" => menu::visual_thumbnail_missing(request),
        "menu.visual_label_missing" => menu::visual_label_missing(request),
        "menu.visual_expected_index_cells" => menu::visual_expected_index_cells(request),
        "menu.parse_batch_frame_stats" => menu::parse_batch_frame_stats(request),
        "menu.parse_index_batch" => menu::parse_index_batch(request),
        "menu.parse_overlay_batch" => menu::parse_overlay_batch(request),
        "path.safe_basename" => config::safe_basename(request),
        "identity.equal"
        | "cache.mlp_match"
        | "resume.signature_match"
        | "prepare.cache_reusable"
        | "disk.group_requirements" => workflow::dispatch(operation, request),
        "audio.parameters" | "metadata.parse" | "decode.scan" => {
            probes::dispatch(operation, request)
        }
        "author.normalize_title_mode" | "author.title_ends" => author::dispatch(operation, request),
        "aob.pts_statistics" | "aob.scan_file" | "aob.audit_diagnostics" => {
            aob::dispatch(operation, request)
        }
        "hash.sha256_hex" | "hash.sha256_file" => hash::dispatch(operation, request),
        "identity.compute" => identity::dispatch(request),
        "publish.set" | "publish.single" | "publish.stage" | "publish.copy" => {
            publication::dispatch(operation, request)
        }
        "lpcm.validate_format" | "lpcm.validate_layout" => lpcm::dispatch(operation, request),
        "mlp.major_sync_interval" => validation::dispatch(operation, request),
        "abi.version" => Ok(json!({"version":1,"backend":"rust"})),
        _ => Err(format!("Unsupported Rust operation: {operation}")),
    }
}
