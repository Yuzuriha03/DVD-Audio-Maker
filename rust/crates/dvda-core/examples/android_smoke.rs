use serde_json::json;

fn main() {
    let defaults = dvda_core::options::defaults("/data/local/tmp");
    println!("default profile keys={}", defaults.len());

    let plan = dvda_core::planner::plan(json!({
        "DiscBytes": 4_707_319_808i64,
        "GroupTrackLimit": 99,
        "Tracks": [
            {"Album":"A","Title":"A1","MlpPath":"1.mlp","MlpSize":10_000_000i64,"SampleRate":48000,"Bits":24},
            {"Album":"A","Title":"A2","MlpPath":"2.mlp","MlpSize":11_000_000i64,"SampleRate":48000,"Bits":24},
            {"Album":"B","Title":"B1","MlpPath":"3.mlp","MlpSize":12_000_000i64,"SampleRate":96000,"Bits":24},
            {"Album":"C","Title":"C1","MlpPath":"4.mlp","MlpSize":13_000_000i64,"SampleRate":48000,"Bits":16}
        ]
    }))
    .expect("planner");
    let discs = plan["Discs"].as_array().unwrap();
    println!(
        "discs={} groups_per_disc={:?} limit={}",
        discs.len(),
        discs
            .iter()
            .map(|disc| disc["Groups"].as_array().map_or(0, Vec::len))
            .collect::<Vec<_>>(),
        plan["DiscContentLimitBytes"]
    );

    let menu = dvda_core::menu::plan(json!({
        "Disc": {
            "Number": 1,
            "Groups": [
                {"Tracks":[
                    {"Title":"A1","Album":"A","Track":"1"},
                    {"Title":"A2","Album":"A","Track":"2"},
                    {"Title":"B1","Album":"B","Track":"1"},
                    {"Title":"C1","Album":"C","Track":"1"},
                    {"Title":"D1","Album":"D","Track":"1"},
                    {"Title":"E1","Album":"E","Track":"1"}
                ]}
            ]
        },
        "Title": "Smoke",
        "Language": "zh-CN",
        "MenuTracksPerPage": 12,
        "MenuIndexMinimumAlbums": 4
    }))
    .expect("menu plan");
    println!(
        "menu pages={} index_pages={}",
        menu["TotalPages"], menu["IndexPages"]
    );
    let options = dvda_core::app::AppOptions::load(None).expect("app options");
    println!(
        "app options language={} title={:?} disc_bytes={:?} mlp_source={:?}",
        options.language,
        options
            .values
            .get("Title")
            .and_then(|value| value.as_str())
            .unwrap_or_default(),
        options.values.get("DiscBytes"),
        options
            .values
            .get("MlpSource")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
    );
    println!("android smoke ok");
}
