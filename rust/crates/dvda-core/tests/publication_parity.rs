//! Regression cases from the former DiscPublisher contract, using real files.
use dvda_core::dispatch;
use serde_json::{Value, json};
use std::{
    fs,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

struct Workspace(PathBuf);
impl Workspace {
    fn new(name: &str) -> std::io::Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root)?;
        Ok(Self(root))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn publication_copy_stage_collision_and_locked_destination() {
    let work = Workspace::new("publication-parity").unwrap();
    let source = work.path().join("source.iso");
    let destination = work.path().join("copy.iso");
    let directory = work.path().join("final");
    fs::write(&source, [1, 2, 3, 4]).unwrap();
    let result = dispatch(
        "publish.copy",
        json!({"Source":source,"Destination":destination}),
    )
    .unwrap();
    assert!(result["Failure"].is_null(), "{result}");
    assert_eq!(fs::read(&source).unwrap(), fs::read(&destination).unwrap());
    fs::create_dir(&directory).unwrap();
    let old = directory.join("disc.iso");
    fs::write(&old, b"old").unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&old)
        .unwrap();
    let result = dispatch(
        "publish.single",
        json!({"Source":source,"Directory":directory,"Name":"disc.iso"}),
    )
    .unwrap();
    assert!(result["Failure"].is_null(), "{result}");
    assert!(result["Value"]["Warning"].is_string());
    assert_eq!(fs::read(&old).unwrap(), b"old");
    assert_eq!(
        fs::read(directory.join("disc_new.iso")).unwrap(),
        [1, 2, 3, 4]
    );
    drop(locked);
    let result = dispatch(
        "publish.stage",
        json!({"Source":source,"Directory":directory,"Name":"disc.iso"}),
    )
    .unwrap();
    assert!(!result["Failure"].is_null());
    assert!(source.is_file(), "failed stage must retain source");
    let result = dispatch(
        "publish.stage",
        json!({"Source":source,"Directory":directory,"Name":"staged.iso"}),
    )
    .unwrap();
    assert!(result["Failure"].is_null(), "{result}");
    assert!(!source.exists());
    assert_eq!(
        fs::read(directory.join("staged.iso")).unwrap(),
        [1, 2, 3, 4]
    );
}

#[test]
fn publication_set_rolls_back_all_outputs_and_moved_sources() {
    for move_files in [false, true] {
        let work = Workspace::new("publication-set-parity").unwrap();
        let root = work.path();
        let final_dir = root.join("final");
        fs::create_dir(&final_dir).unwrap();
        let pending = root.join("pending.json");
        let formal = final_dir.join("index.json");
        fs::write(&pending, b"new index").unwrap();
        fs::write(&formal, b"old index").unwrap();
        let files: Vec<Value> = (1..=2)
            .map(|n| {
                let name = format!("disc{n}.iso");
                let source = root.join(&name);
                fs::write(&source, format!("new {n}")).unwrap();
                fs::write(final_dir.join(&name), format!("old {n}")).unwrap();
                json!({"SourcePath":source,"FileName":name})
            })
            .collect();
        let request = json!({"IsoFiles":files,"FinalDirectory":final_dir,"PendingIndexPath":pending,"FormalIndexPath":formal,"MoveStagedIsos":move_files});
        // A late failure commits both ISOs before failing at the index. Rollback
        // must restore the complete previous set, and move mode's source ISOs.
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&formal)
            .unwrap();
        let result = dispatch("publish.set", request.clone()).unwrap();
        assert!(!result["Failure"].is_null(), "{result}");
        for n in 1..=2 {
            assert_eq!(
                fs::read(root.join(format!("disc{n}.iso"))).unwrap(),
                format!("new {n}").as_bytes()
            );
            assert_eq!(
                fs::read(final_dir.join(format!("disc{n}.iso"))).unwrap(),
                format!("old {n}").as_bytes()
            );
        }
        assert_eq!(fs::read(&formal).unwrap(), b"old index");
        assert_eq!(fs::read(&pending).unwrap(), b"new index");
        assert_eq!(fs::read_dir(&final_dir).unwrap().count(), 3);
        drop(locked);
        let mut duplicate = request.clone();
        duplicate["IsoFiles"][1]["FileName"] = json!("DISC1.ISO");
        let result = dispatch("publish.set", duplicate).unwrap();
        assert_eq!(result["Failure"]["Kind"], "InvalidData");
        let result = dispatch("publish.set", request).unwrap();
        assert!(result["Failure"].is_null(), "{result}");
        assert_eq!(result["Value"].as_array().unwrap().len(), 2);
        assert_eq!(fs::read(&formal).unwrap(), b"new index");
        assert!(!pending.exists());
        for n in 1..=2 {
            assert_eq!(root.join(format!("disc{n}.iso")).exists(), !move_files);
            assert_eq!(
                fs::read(final_dir.join(format!("disc{n}.iso"))).unwrap(),
                format!("new {n}").as_bytes()
            );
        }
        assert_eq!(fs::read_dir(&final_dir).unwrap().count(), 3);
    }
}
