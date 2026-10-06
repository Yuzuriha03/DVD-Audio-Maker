use dvda_core::{dispatch, options};
use serde_json::{Value, json};

fn evaluate(file: Value, environment: Value) -> Value {
    options::evaluate(options::Request {
        config_path: Some("C:/工作/settings.json".into()),
        file_values: file.as_object().unwrap().clone(),
        environment: environment.as_object().unwrap().clone(),
        defaults: options::defaults("C:/Local"),
        positive_sign: "+".into(),
        negative_sign: "-".into(),
    })
}

#[test]
fn option_precedence_sources_numeric_fallbacks_and_derived_paths() {
    let result = evaluate(
        json!({"DVDA_SRC":"C:/file/","DVDA_FINAL_DIR":"D:/out/","DVDA_BUILD_DIR":"D:/work/","DVDA_TITLE":"鸣潮 DVD-Audio 2026","DVDA_DISC_BYTES":"bad","CUSTOM_VALUE":"fixture"}),
        json!({"DVDA_SRC":"C:/env/","DVDA_TITLE":""}),
    );
    let p = &result["Properties"];
    for (key, value) in [
        ("SourceDirectory", "C:/env"),
        ("FinalDirectory", "D:/out"),
        ("IsoPrefix", "DVD_Audio_2026"),
        ("ManifestPath", "D:/work\\manifest.json"),
        ("MlpExternalDirectory", "D:/work\\mlp"),
    ] {
        assert_eq!(p[key]["Value"], value);
    }
    assert_eq!(p["DiscBytes"]["Value"], 4_707_319_808_i64);
    assert_eq!(p["PlannedDiscs"]["Value"], 0);
    assert_eq!(result["Sources"]["DVDA_SRC"], "环境变量");
    assert_eq!(result["Sources"]["DVDA_TITLE"], "settings.json");
    assert_eq!(result["Sources"]["CUSTOM_VALUE"], "settings.json");
    assert_eq!(result["Sources"]["DVDA_MENU"], "默认值");
    assert_eq!(result["Overrides"], json!(["DVDA_SRC"]));
    assert_eq!(result["MissingRequired"], json!([]));
    let empty = evaluate(json!({}), json!({"DVDA_SRC":"","DVDA_FINAL_DIR":""}));
    assert_eq!(
        empty["MissingRequired"],
        json!(["DVDA_SRC", "DVDA_FINAL_DIR"])
    );
    assert_eq!(
        empty["Properties"]["BuildDirectory"]["Value"],
        "C:/Local\\DVD-Audio-Maker\\build"
    );
    assert_eq!(
        dispatch("shell.assignment", json!({"Key":"KEY","Value":"a'b\n中文"})).unwrap(),
        "KEY='a'\\''b\n中文'"
    );
}

#[test]
fn option_ranges_sources_and_removed_branches() {
    for (value, expected) in [
        ("500", 99),
        ("0", 1),
        ("bad", 99),
        ("+30", 30),
        ("2147483648", 99),
    ] {
        assert_eq!(
            evaluate(json!({"DVDA_GROUP_TRACK_LIMIT":value}), json!({}))["Properties"]["GroupTrackLimit"]
                ["Value"],
            expected
        );
    }
    for (value, expected) in [
        (" EXTERNAL ", "external"),
        ("SURCODE", "external"),
        ("batch-surcode", "surcode-batch"),
        ("SURCODE-BATCH", "surcode-batch"),
        ("LPCM", "lpcm"),
    ] {
        let r = evaluate(json!({"DVDA_MLP_SOURCE":value}), json!({}));
        assert_eq!(r["Properties"]["MlpSource"]["Value"], expected);
        assert!(r["Properties"]["MlpSource"]["Error"].is_null());
    }
    for value in ["ffmpeg", "unknown"] {
        assert!(evaluate(json!({"DVDA_MLP_SOURCE":value}),json!({}))["Properties"]["MlpSource"]["Error"].is_string());
    }
    let result = evaluate(
        json!({"DVDA_MLP_JOBS":"999","DVDA_MENU_TRACKS_PER_PAGE":"0","DVDA_MENU_COVER_DIM":"-1","DVDA_DISC_BYTES":"0","DVDA_RESUME":"off","DVDA_MENU":"YES","DVDA_LOSS_ERROR_S":"not a number"}),
        json!({}),
    );
    let p = &result["Properties"];
    assert_eq!(p["MlpJobs"]["Value"], 16);
    assert_eq!(p["MenuTracksPerPage"]["Value"], 1);
    assert_eq!(p["MenuCoverDim"]["Value"], 0);
    assert_eq!(p["DiscBytes"]["Value"], 4_707_319_808_i64);
    assert_eq!(p["ResumeEnabled"]["Value"], false);
    assert_eq!(p["MenuEnabled"]["Value"], true);
    assert_eq!(
        p["LossErrorSeconds"]["Value"].as_i64().unwrap() as u64,
        0.05_f64.to_bits()
    );
}

#[test]
fn default_mlp_jobs_tracks_available_parallelism() {
    let jobs = options::default_mlp_jobs();
    assert!((1..=16).contains(&jobs));
    assert_eq!(
        options::defaults("C:/Local")["DVDA_MLP_JOBS"].as_str(),
        Some("auto")
    );
    assert_eq!(
        evaluate(json!({"DVDA_MLP_JOBS":"auto"}), json!({}))["Properties"]["MlpJobs"]["Value"],
        0
    );
}
