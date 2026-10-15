use reader_core::appearance::{AppearanceLinkage, AutomaticSource, DisplayChoice};

#[test]
fn shared_platform_contract() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/appearance-linkage.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let mut value: AppearanceLinkage = serde_json::from_value(case["initial"].clone()).unwrap();
        let mut system = case["system"].as_bool().unwrap();
        let mut ambient = case["ambient"].as_bool();
        for action in case["actions"].as_array().unwrap() {
            let action = action.as_str().unwrap();
            let (key, arg) = action.split_once(':').unwrap_or((action, ""));
            match key {
                "source" => {
                    value.source = if arg == "ambient" {
                        AutomaticSource::Ambient
                    } else {
                        AutomaticSource::System
                    }
                }
                "system" => system = arg == "dark",
                "ambient" => ambient = Some(arg == "dark"),
                "appLink" => value.toggle_app_link(system, ambient),
                "readingLink" => value.toggle_reading_link(system, ambient),
                "app" => value.set_app(arg == "dark"),
                "reading" => {
                    value.reading_mode = match arg {
                        "light" => DisplayChoice::Light,
                        "dark" => DisplayChoice::Dark,
                        _ => DisplayChoice::Automatic,
                    }
                }
                "readingSource" => {
                    value.reading_source = if arg == "ambient" {
                        AutomaticSource::Ambient
                    } else {
                        AutomaticSource::System
                    }
                }
                "shortcut" => value.toggle_reading(system, ambient),
                _ => panic!("unknown fixture action {action}"),
            }
            // Every transition must survive persistence; live sensor data is not persisted.
            value = serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        }
        let actual = serde_json::json!({"app":value.app_dark(system,ambient),"reading":value.reading_dark(system,ambient),"appLinked":value.app_linked,"readingLinked":value.reading_linked});
        assert_eq!(actual, case["expected"], "{}", case["name"]);
    }
}

#[test]
fn legacy_migration_keeps_both_surfaces() {
    for app in [false, true] {
        for reading in [false, true] {
            let value = AppearanceLinkage::migrate(app, reading);
            assert_eq!(value.app_dark(!app, None), app);
            assert_eq!(value.reading_dark(!app, None), reading);
        }
    }
}
