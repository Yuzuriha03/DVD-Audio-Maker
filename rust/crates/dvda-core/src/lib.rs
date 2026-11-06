#[allow(
    clippy::needless_pass_by_value,
    reason = "Acquisition helpers retain owned JSON requests for the common dispatch contract"
)]
pub mod acquisition;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    reason = "AOB verification retains bounded sector arithmetic and complete diagnostic reporting"
)]
pub mod aob;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    reason = "Application options preserve validated configuration sizes and owned request values"
)]
pub mod app;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    reason = "Audio inspection retains media ABI widths and callback-based error contracts"
)]
pub mod audio;
#[allow(
    clippy::needless_pass_by_value,
    reason = "Author adapters retain the common owned-request dispatch signatures"
)]
pub mod author;
#[allow(
    clippy::items_after_statements,
    reason = "Runtime helpers intentionally scope imports to their backend branches"
)]
pub mod author_runtime;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::items_after_statements,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "Batch processing retains ordered worker state and bounded track metadata"
)]
pub mod batch;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::needless_pass_by_value,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    reason = "Build orchestration retains complete transactions and explicit configuration flags"
)]
pub mod build;
#[allow(
    clippy::needless_pass_by_value,
    reason = "Build log helpers preserve the common owned-request dispatch contract"
)]
pub mod buildlog;
#[allow(
    clippy::needless_pass_by_value,
    reason = "Cache helpers retain owned JSON requests for the common dispatch contract"
)]
pub mod cache;
#[allow(
    clippy::needless_pass_by_value,
    reason = "Configuration helpers retain owned JSON values for the dispatch contract"
)]
pub mod config;
#[allow(
    clippy::items_after_statements,
    clippy::needless_pass_by_value,
    reason = "Profile loading retains scoped parser imports and owned profile values"
)]
pub mod config_files;
pub mod conversion;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "Disc planning retains validated capacity arithmetic and complete grouping rules"
)]
pub mod disc;
pub mod disk_space;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::similar_names,
    clippy::too_many_lines,
    reason = "Encoder adapters preserve PCM widths, metadata sizes, and stream ownership"
)]
pub mod encoder;
#[allow(
    clippy::cast_possible_truncation,
    clippy::unnecessary_wraps,
    reason = "Encoder stamp generation preserves fixed packet fields and a shared fallible API"
)]
mod encoder_stamp;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "Format readers retain protocol field widths and owned parser requests"
)]
pub mod formats;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::needless_pass_by_value,
    reason = "Hash helpers retain owned requests and benchmark fixtures"
)]
pub mod hash;
#[allow(
    clippy::needless_pass_by_value,
    reason = "Identity dispatch retains owned requests"
)]
pub mod identity;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::missing_errors_doc,
    reason = "IFO parsing retains protocol-sized group and sector fields"
)]
pub mod ifo;
pub mod images;
#[allow(
    clippy::needless_pass_by_value,
    reason = "Index generation retains owned track records"
)]
pub mod index;
#[allow(
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "LPCM preparation retains validated audio geometry and streaming state"
)]
pub mod lpcm;
#[allow(
    clippy::cast_possible_wrap,
    clippy::needless_pass_by_value,
    reason = "Manifest parsing retains owned paths and protocol-sized indices"
)]
pub mod manifest;
pub mod media;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::items_after_statements,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "Menu planning retains text quantization, owned JSON values, and complete layout rules"
)]
pub mod menu;
#[allow(
    clippy::cast_possible_truncation,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    reason = "Menu checks retain bounded geometry and incremental XML diagnostics"
)]
pub mod menu_check;
#[allow(
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "Font resolution retains complete coverage and runtime fallback checks"
)]
pub mod menu_fonts;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::too_many_lines,
    reason = "Menu verification retains sector arithmetic and complete navigation checks"
)]
pub mod menu_verify;
mod metrics;
#[allow(
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::trivially_copy_pass_by_ref,
    reason = "MLP import retains shared native format arguments and owned track records"
)]
pub mod mlp_import;
#[cfg(feature = "rust-mlp")]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    reason = "Streaming MLP conversion retains bounded PCM geometry and direct callback state"
)]
mod mlp_stream;
#[allow(
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    reason = "MLP workflow retains cache evidence, native format arguments, and complete publication stages"
)]
pub mod mlp_workflow;
mod native_components;
#[allow(
    clippy::cast_possible_wrap,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "Option evaluation retains signed configuration values and complete precedence rules"
)]
pub mod options;
mod parallel;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::items_after_statements,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "PCM normalization retains validated WAVE geometry and streaming output state"
)]
pub mod pcm;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "Planning helpers retain owned JSON requests and documented capacity rounding"
)]
pub mod planner;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::default_trait_access,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::similar_names,
    clippy::too_many_lines,
    reason = "Preparation retains ALAC bit positions, snapshot values, and streaming worker state"
)]
pub mod preparation;
#[allow(
    clippy::cast_precision_loss,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    reason = "Probe dispatch retains owned requests and floating-point diagnostic formatting"
)]
pub mod probes;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::needless_pass_by_value,
    reason = "Process adapters preserve native status widths and owned request values"
)]
pub mod process;
#[allow(
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    reason = "Publication retains explicit transaction flags and complete rollback state"
)]
pub mod publication;
#[allow(
    clippy::needless_pass_by_value,
    reason = "Resume records retain owned task data"
)]
pub mod resume;
#[allow(
    clippy::cast_possible_truncation,
    clippy::items_after_statements,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    reason = "Runtime archive handling retains bounded headers and scoped platform imports"
)]
pub mod runtime;
#[allow(
    clippy::needless_pass_by_value,
    reason = "Signature helpers retain owned evidence for the dispatch contract"
)]
pub mod signature;
#[allow(
    clippy::needless_pass_by_value,
    reason = "Task log records intentionally own opaque text and paths"
)]
pub mod task_log;
#[allow(
    clippy::needless_pass_by_value,
    reason = "Validation helpers retain owned dispatch requests"
)]
pub mod validation;
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    clippy::needless_pass_by_value,
    reason = "Verification helpers retain normalized ISO names and owned audit records"
)]
pub mod verification;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::default_trait_access,
    clippy::needless_pass_by_value,
    clippy::similar_names,
    clippy::too_many_lines,
    reason = "Disc verification retains sector arithmetic, normalized names, and complete evidence collection"
)]
pub mod verify;
#[allow(
    clippy::missing_errors_doc,
    clippy::too_many_lines,
    reason = "Audio verification retains complete per-track comparison stages"
)]
pub mod verify_audio;
#[allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    reason = "Streaming comparison retains bounded offsets and owned track requests"
)]
mod verify_audio_stream;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::missing_errors_doc,
    clippy::needless_pass_by_value,
    reason = "Workflow evidence retains owned JSON values and capacity estimates"
)]
pub mod workflow;
#[allow(
    clippy::items_after_statements,
    clippy::missing_errors_doc,
    reason = "Workspace lifecycle retains scoped imports and explicit retry control flow"
)]
pub mod workspace;

use serde_json::{Value, json};

/// Dispatch a JSON compatibility operation to its typed Rust implementation.
///
/// # Errors
/// Returns an error for unsupported operations, invalid request values, or an underlying operation failure.
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

#[allow(
    clippy::cast_possible_truncation,
    clippy::items_after_statements,
    clippy::match_wildcard_for_single_variants,
    reason = "Localization retains complete translation tables and scoped language helpers"
)]
pub mod localization;
