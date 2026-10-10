pub mod acquisition;
pub mod aob;
pub mod app;
pub mod audio;
pub mod author;
pub mod batch;
pub mod build;
pub mod buildlog;
pub mod cache;
pub mod config;
pub mod config_files;
pub mod conversion;
pub mod disc;
pub mod disk_space;
pub mod encoder;
pub mod formats;
pub mod hash;
pub mod identity;
pub mod ifo;
pub mod images;
pub mod index;
pub mod lpcm;
pub mod manifest;
pub mod media;
pub mod menu;
pub mod menu_check;
pub mod menu_fonts;
pub mod menu_verify;
pub mod mlp_import;
pub mod mlp_workflow;
mod native_components;
pub mod options;
pub mod pcm;
pub mod planner;
pub mod preparation;
pub mod probes;
pub mod process;
pub mod publication;
pub mod resume;
pub mod runtime;
pub mod signature;
pub mod task_log;
pub mod validation;
pub mod verification;
pub mod verify;
pub mod verify_audio;
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
        "profile.load" | "profile.save" => config_files::dispatch(operation, request),
        "disc.plan" => planner::plan(request),
        "build.disc_plan" => disc::dispatch(request),
        "build.limit_albums" => disc::limit_albums(request),
        "build.mlp_lookup" | "build.mlp_resolve" | "build.mlp_destination" => {
            acquisition::dispatch(operation, request)
        }
        "mlp.acquire" => Err("mlp.acquire requires the typed Rust host ABI".into()),
        "build.execute" => Err("build.execute requires the typed Rust host ABI".into()),
        "build.log_header" => buildlog::dispatch(operation, request),
        "build.disc_signature" => signature::dispatch(operation, request),
        "cache.load" | "cache.save" | "resume.load" | "resume.save" => {
            cache::dispatch(operation, request)
        }
        "build.write_index" => index::dispatch(operation, request),
        "build.read_manifest" => manifest::dispatch(request),
        "prepare.normalize_albums"
        | "prepare.roundtrip"
        | "prepare.snapshot_reuse"
        | "prepare.snapshot_save"
        | "prepare.enumerate" => preparation::snapshot::dispatch(operation, request),
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
        "menu.plan" => menu::plan(request),
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
        | "disk.group_requirements"
        | "disk.estimate_requirements" => workflow::dispatch(operation, request),
        "audio.parameters" | "metadata.parse" | "decode.scan" => {
            probes::dispatch(operation, request)
        }
        "author.normalize_title_mode" | "author.title_ends" | "author.build_args" => {
            author::dispatch(operation, request)
        }
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
        "verify.parse_audit_log" | "verify.matches_iso_name" => {
            verification::dispatch(operation, request)
        }
        "verify.execute" => Err("verify.execute requires the typed Rust host ABI".into()),
        "encoder.write_metadata" => encoder::dispatch(operation, request),
        "abi.version" => Ok(json!({"version":1,"backend":"rust"})),
        _ => Err(format!("Unsupported Rust operation: {operation}")),
    }
}

pub mod localization;
