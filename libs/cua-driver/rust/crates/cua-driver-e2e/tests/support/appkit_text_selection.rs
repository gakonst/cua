// SPDX-License-Identifier: MIT
//! Native selection effects are asserted using the fixture's NSTextView state,
//! not the value or selection echoed by the Driver response.

use super::*;
use serde_json::{json, Value};

fn state(directory: &Path, accept: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(data) = std::fs::read(directory.join("state.json")) {
            let value: Value = serde_json::from_slice(&data).expect("parse selection oracle");
            if accept(&value) {
                return value;
            }
        }
        assert!(
            Instant::now() < deadline,
            "selection fixture did not reach the expected state"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn command(directory: &Path, sequence: u64, mode: &str) {
    let path = directory.join("command.tmp");
    std::fs::write(
        &path,
        json!({"sequence":sequence,"mode":mode,"reset":true}).to_string(),
    )
    .unwrap();
    std::fs::rename(path, directory.join("command.json")).unwrap();
    state(directory, |s| s["sequence"] == sequence);
}

#[test]
#[ignore]
fn harness_appkit_select_text_matches_native_selection_and_refuses_uncertainty() {
    let case = native_background_case(
        "appkit",
        "select_text",
        Targeting::Ax,
        DriverRoute::MacosAxValue,
    );
    execute_case(case, |evidence| {
        let directory = tempfile::tempdir().expect("selection fixture directory");
        let path = directory.path().to_str().unwrap();
        let harness = Harness::spawn(
            None,
            None,
            false,
            None,
            &[("CUA_APPKIT_TEXT_SELECTION_DIR", path)],
        )
        .presented();
        let initial = state(directory.path(), |_| true);
        let window = initial["primary_window"].as_u64().unwrap();
        let seed = initial["primary"]["value"].as_str().unwrap();
        let sibling = initial["sibling"].clone();
        let mut driver = McpDriver::spawn_macos_daemon_proxy_named("appkit-select-text")
            .expect("selection-enabled daemon must be running");
        *evidence = recording_evidence(driver.recording_dir());
        let target = TargetWindow {
            pid: harness.pid,
            native_id: window,
        };
        let (_, passed) = run_with_background_oracles(&mut driver,target,|driver| {
            let snapshot = snapshot_elements(driver,harness.pid,window);
            assert!(!snapshot.is_error(),"{}",snapshot.text());
            let token = element_token_by_id(&snapshot,"selection-editor");
            for (text, context) in [("world",None),("café",None),("e\u{301}",None),("👍🏽",None),("cat",Some(" ran"))] {
                for kind in ["text","cursor_before","cursor_after"] {
                    let byte = if context.is_some() { seed.find("cat ran").unwrap() } else { seed.find(text).unwrap() };
                    let location = seed[..byte].encode_utf16().count();
                    let size = text.encode_utf16().count();
                    let range = json!({"location":if kind=="cursor_after" {location+size} else {location},"length":if kind=="text" {size} else {0}});
                    let mut args = json!({"element_token":token,"text":text,"selection_type":kind});
                    if let Some(suffix) = context { args["suffix"] = json!(suffix); }
                    let result = driver.call("select_text",args);
                    assert!(!result.is_error(),"selection failed: {}",result.text());
                    assert_eq!(result.structured()["verified"],true);
                    assert_eq!(result.structured()["range"],range);
                    let observed = state(directory.path(),|s| s["primary"]["range"]==range);
                    assert_eq!(observed["primary"]["value"],seed);
                    assert_eq!(observed["primary"]["selected_text"],if kind=="text" {text} else {""});
                    assert_eq!(observed["sibling"],sibling,"selection reached the sibling editor");
                }
            }
            for (text,code) in [("cat","ambiguous_text_match"),("absent","text_not_found")] {
                let before = state(directory.path(),|_|true)["primary"].clone();
                let refused = driver.call("select_text",json!({"element_token":token,"text":text}));
                assert!(refused.is_error());
                assert_eq!(refused.structured()["code"],code);
                assert_eq!(refused.structured()["effect"],"refused");
                assert_eq!(state(directory.path(),|_|true)["primary"],before);
            }
            let button = element_token_by_id(&snapshot,"selection-button");
            let refused = driver.call("select_text",json!({"element_token":button,"text":"button"}));
            assert!(refused.is_error());
            assert_eq!(refused.structured()["code"],"selection_not_settable");

            // Application-side adversaries: a successful AX setter is not enough.
            for (index,mode) in ["ignore","mutate","misreport"].iter().enumerate() {
                command(directory.path(),index as u64+1,mode);
                let result = driver.call("select_text",json!({"element_token":token,"text":"world"}));
                assert!(result.is_error(),"false confirmation for {mode}: {}",result.text());
                assert_eq!(result.structured()["code"],"selection_not_applied");
                assert_eq!(result.structured()["effect"],"unverified");
                assert_eq!(result.structured()["verified"],false);
            }
            command(directory.path(),4,"normal");
            let newer = snapshot_elements(driver,harness.pid,window);
            assert_ne!(snapshot.snapshot_id(),newer.snapshot_id());
            let refused = driver.call("select_text",json!({"pid":harness.pid,"element_token":token,"text":"world"}));
            assert!(refused.is_error());
            assert_eq!(refused.structured()["refusal"]["code"],"stale_element_token");
        }).unwrap_or_else(|error| panic!("background selection contract: {error}"));
        Observation::delivered_with_fixture_state(passed)
    });
}
