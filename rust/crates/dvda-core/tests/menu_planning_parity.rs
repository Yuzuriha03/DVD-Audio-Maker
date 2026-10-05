//! Frozen expectations copied from the former menu compatibility tests.
use dvda_core::dispatch;
use serde_json::{Value, json};
fn run(name: &str, value: Value) -> Value {
    dispatch(name, value).unwrap()
}

#[test]
fn menu_pages_text_and_multi_index_cells_match_historical_examples() {
    for (input, expected) in [
        ("繁星、新生,与你:终章=一", "繁星、新生，与你：终章＝一"),
        ("A,B:C=D", "A·B·C·D"),
    ] {
        assert_eq!(
            run("menu.sanitize", json!(input)),
            json!({"Value":expected,"Changed":true})
        );
    }
    assert_eq!(
        run("menu.sanitize", json!("plain")),
        json!({"Value":"plain","Changed":false})
    );
    assert_eq!(
        run(
            "menu.short_album",
            json!({"Value":"Album(Original Soundtrack)"})
        ),
        "Album"
    );
    assert!(
        run(
            "menu.truncate",
            json!({"Value":"中".repeat(100),"FontSize":20})
        )
        .as_str()
        .unwrap()
        .ends_with('~')
    );
    let tracks:Vec<_>=[("A1","Album A"),("A2","Album A"),("A3","Album A"),("B1","Album B"),("C1","Album C")].into_iter().map(|(title,album)|json!({"Title":title,"Album":album,"SourcePath":format!("C:/Music/{album}/{title}.flac")})).collect();
    let value = run(
        "menu.plan",
        json!({"Disc":{"Groups":[{"Tracks":tracks}]},"Title":"Disc,Title","MenuTracksPerPage":2,"MenuIndexMinimumAlbums":3}),
    );
    assert_eq!(value["AlbumPages"].as_array().unwrap().len(), 4);
    assert_eq!(value["IndexPages"], 1);
    assert_eq!(value["TotalPages"], 5);
    assert_eq!(value["TrackCount"], 5);
    assert_eq!(value["AlbumPages"][1]["Continuation"], true);
    assert!(
        value["ScreenText"]
            .as_str()
            .unwrap()
            .starts_with("Disc·Title=选择专辑=")
    );
    for (page, expected) in [(0, 12), (1, 12), (2, 1), (3, 0)] {
        assert_eq!(
            run(
                "menu.visual_expected_index_cells",
                json!({"AlbumCount":25,"PageIndex":page,"PerPage":12})
            ),
            expected
        );
    }
    assert!(dispatch("menu.pages", json!({"Tracks":[],"Albums":[],"RowCap":0})).is_err());
}

#[test]
fn menu_visual_thresholds_and_nonfinite_statistics_match_historical_examples() {
    for (value, expected) in [(75, false), (160, false), (161, true), (238, true)] {
        assert_eq!(
            run("menu.visual_background_invalid", json!(value)),
            expected
        );
    }
    for (value, expected) in [(3, true), (4, false), (116, false)] {
        assert_eq!(run("menu.visual_thumbnail_missing", json!(value)), expected);
    }
    for (maximum, mean, expected) in [(240, 80, false), (150, 80, true), (240, 238, true)] {
        assert_eq!(
            run(
                "menu.visual_label_missing",
                json!({"Maximum":maximum,"Mean":mean})
            ),
            expected
        );
    }
    for (sd, colors, expected) in [(0.5, 1000, true), (20., 20, true), (20., 1000, false)] {
        assert_eq!(
            run(
                "menu.visual_near_solid",
                json!({"StandardDeviation":sd,"Colors":colors})
            ),
            expected
        );
    }
    for (value, valid) in [
        ("  +1.25e2 ", true),
        ("-0", true),
        ("1e309", false),
        ("NaN", false),
        ("Infinity", false),
        ("1,234", false),
        ("1_000", false),
        ("\u{a0}125\u{a0}", false),
        ("125\0", true),
        (".5", true),
        ("5.", true),
        ("0e9999", true),
    ] {
        assert_eq!(
            !run(
                "menu.parse_batch_frame_stats",
                json!(format!("F|{value}|1|2\r\n"))
            )
            .is_null(),
            valid,
            "{value:?}"
        );
    }
    assert!(run("menu.parse_batch_frame_stats", json!("F|1|2|3\nF|1|2|3\n")).is_null());
}
