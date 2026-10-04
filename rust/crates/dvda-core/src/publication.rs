//! ISO publication and rollback. This module owns all transaction filesystem changes.
use dvda_native::files::{copy_file, move_file, ordinal_ignore_case};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Failure {
    kind: &'static str,
    code: Option<i32>,
    message: String,
}
impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self {
            kind: if error.kind() == io::ErrorKind::PermissionDenied {
                "Access"
            } else {
                "Io"
            },
            code: error.raw_os_error(),
            message: error.to_string(),
        }
    }
}
type Result<T> = std::result::Result<T, Failure>;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct IsoFile {
    source_path: String,
    file_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SetRequest {
    iso_files: Vec<IsoFile>,
    final_directory: String,
    pending_index_path: String,
    formal_index_path: String,
    move_staged_isos: bool,
}

struct Item {
    source: PathBuf,
    destination: PathBuf,
    temporary: PathBuf,
    backup: PathBuf,
    move_source: bool,
    source_moved: bool,
    backup_created: bool,
    committed: bool,
}

fn suffix(path: &Path, text: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(text);
    PathBuf::from(name)
}
fn token() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{:x}-{:x}-{:x}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}
fn root(path: &Path) -> io::Result<String> {
    let absolute = std::path::absolute(path)?;
    let mut root = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => root.push(component.as_os_str()),
            _ => break,
        }
    }
    Ok(root.to_string_lossy().into_owned())
}
fn same_root(left: &Path, right: &Path) -> io::Result<bool> {
    Ok(ordinal_ignore_case(&root(left)?, &root(right)?))
}
fn try_delete(path: &Path) {
    if path.is_file() {
        let _ = fs::remove_file(path);
    }
}
fn copy_temporary(source: &Path, temporary: &Path) -> Result<()> {
    try_delete(temporary);
    copy_file(source, temporary, true)?;
    let source_size = fs::metadata(source)?.len();
    let copied_size = fs::metadata(temporary)?.len();
    if source_size != copied_size {
        try_delete(temporary);
        return Err(Failure {
            kind: "Io",
            code: None,
            message: format!("文件复制长度不一致: {source_size} != {copied_size}"),
        });
    }
    Ok(())
}
fn copy_verified(source: &Path, destination: &Path) -> Result<()> {
    let temporary = suffix(destination, ".copying");
    copy_temporary(source, &temporary)?;
    move_file(&temporary, destination, true)?;
    Ok(())
}

fn publish_set(request: SetRequest) -> Result<Value> {
    let mut groups: Vec<(&str, usize)> = Vec::new();
    for file in &request.iso_files {
        if let Some(group) = groups
            .iter_mut()
            .find(|(name, _)| ordinal_ignore_case(name, &file.file_name))
        {
            group.1 += 1;
        } else {
            groups.push((&file.file_name, 1));
        }
    }
    let duplicates: Vec<_> = groups
        .iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, _)| *name)
        .collect();
    if !duplicates.is_empty() {
        return Err(Failure {
            kind: "InvalidData",
            code: None,
            message: format!("发布集合包含重复 ISO 文件名: {}", duplicates.join(", ")),
        });
    }
    let final_directory = Path::new(&request.final_directory);
    let formal = Path::new(&request.formal_index_path);
    fs::create_dir_all(final_directory)?;
    fs::create_dir_all(
        formal
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let transaction = token();
    let mut items = Vec::new();
    for file in &request.iso_files {
        let source = PathBuf::from(&file.source_path);
        let destination = final_directory.join(&file.file_name);
        let move_source = request.move_staged_isos && same_root(&source, final_directory)?;
        items.push(Item {
            source,
            temporary: suffix(&destination, &format!(".publishing-{transaction}")),
            backup: suffix(&destination, &format!(".backup-{transaction}")),
            destination,
            move_source,
            source_moved: false,
            backup_created: false,
            committed: false,
        });
    }
    items.push(Item {
        source: PathBuf::from(&request.pending_index_path),
        destination: formal.to_owned(),
        temporary: suffix(formal, &format!(".publishing-{transaction}")),
        backup: suffix(formal, &format!(".backup-{transaction}")),
        move_source: false,
        source_moved: false,
        backup_created: false,
        committed: false,
    });
    let execution = (|| -> Result<()> {
        for item in &mut items {
            if item.move_source {
                move_file(&item.source, &item.temporary, false)?;
                item.source_moved = true;
            } else {
                copy_temporary(&item.source, &item.temporary)?;
            }
        }
        for item in &mut items {
            if item.destination.is_file() {
                move_file(&item.destination, &item.backup, false)?;
                item.backup_created = true;
            }
            move_file(&item.temporary, &item.destination, false)?;
            item.committed = true;
        }
        Ok(())
    })();
    if execution.is_err() {
        for item in items.iter().rev() {
            if item.source_moved {
                let current = if item.committed {
                    &item.destination
                } else {
                    &item.temporary
                };
                if current.is_file() {
                    let _ = move_file(current, &item.source, false);
                }
            } else if item.committed {
                try_delete(&item.destination);
            }
            if item.backup_created && item.backup.is_file() && !item.destination.is_file() {
                let _ = move_file(&item.backup, &item.destination, false);
            }
        }
    }
    // If moving a source back failed, its temporary path can be the only remaining copy.
    for item in &items {
        if !item.source_moved {
            try_delete(&item.temporary);
        }
    }
    execution?;
    for item in &items {
        try_delete(&item.backup);
    }
    try_delete(Path::new(&request.pending_index_path));
    Ok(json!(
        request
            .iso_files
            .iter()
            .map(|file| final_directory
                .join(&file.file_name)
                .to_string_lossy()
                .into_owned())
            .collect::<Vec<_>>()
    ))
}

fn publish(source: &Path, directory: &Path, name: &str) -> Result<Value> {
    fs::create_dir_all(directory)?;
    let destination = directory.join(name);
    match copy_verified(source, &destination) {
        Ok(()) => Ok(json!({"Path": destination, "Warning": null})),
        Err(error) => {
            let alternate = directory.join(format!(
                "{}_new.iso",
                Path::new(name)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
            ));
            copy_verified(source, &alternate)?;
            Ok(json!({"Path": alternate, "Warning": error.message}))
        }
    }
}

pub fn dispatch(operation: &str, request: Value) -> std::result::Result<Value, String> {
    let result = if operation == "publish.set" {
        publish_set(serde_json::from_value(request).map_err(|e| e.to_string())?)
    } else {
        let source = Path::new(request["Source"].as_str().ok_or("Missing source")?);
        if operation == "publish.copy" {
            let destination = Path::new(
                request["Destination"]
                    .as_str()
                    .ok_or("Missing destination")?,
            );
            copy_verified(source, destination).map(|()| Value::Null)
        } else {
            let directory = Path::new(request["Directory"].as_str().ok_or("Missing directory")?);
            let name = request["Name"].as_str().ok_or("Missing file name")?;
            if operation == "publish.single" {
                publish(source, directory, name)
            } else if operation == "publish.stage" {
                (|| -> Result<Value> {
                    if !same_root(source, directory)? {
                        return publish(source, directory, name);
                    }
                    fs::create_dir_all(directory)?;
                    let staged = directory.join(name);
                    move_file(source, &staged, false)?;
                    Ok(json!({"Path": staged, "Warning": null}))
                })()
            } else {
                return Err(format!("Unknown publication operation: {operation}"));
            }
        }
    };
    Ok(match result {
        Ok(value) => json!({"Value": value, "Failure": null}),
        Err(error) => json!({"Value": null, "Failure": error}),
    })
}
