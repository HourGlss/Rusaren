use std::time::Duration;

use game_api::WebRtcRuntimeConfig;
use game_sim::COMBAT_FRAME_MS;

use crate::config::{
    parse_csv_urls, parse_tick_interval, parse_turn_ttl, parse_webrtc_config_from_env,
};
use crate::demo::run_demo;
use crate::logging::LogFormat;

#[test]
fn demo_script_produces_the_expected_vertical_slice_markers() {
    let output = run_demo().expect("demo should run");
    let joined = output.join("\n");

    assert!(joined.contains("launch countdown started"));
    assert!(joined.contains("combat started"));
    assert!(joined.contains("round 1 won by Team A"));
    assert!(joined.contains("NoContest"));
}

#[test]
fn parse_tick_interval_uses_default_for_missing_zero_or_invalid_values() {
    assert_eq!(
        parse_tick_interval(None),
        Duration::from_millis(u64::from(COMBAT_FRAME_MS))
    );
    assert_eq!(
        parse_tick_interval(Some(String::from("0"))),
        Duration::from_millis(u64::from(COMBAT_FRAME_MS))
    );
    assert_eq!(
        parse_tick_interval(Some(String::from("abc"))),
        Duration::from_millis(u64::from(COMBAT_FRAME_MS))
    );
}

#[test]
fn parse_tick_interval_accepts_positive_milliseconds() {
    assert_eq!(
        parse_tick_interval(Some(String::from("25"))),
        Duration::from_millis(25)
    );
}

#[test]
fn parse_csv_urls_discards_blank_entries() {
    assert_eq!(
        parse_csv_urls(Some(String::from(
            "stun:one.example.com:3478, ,turn:two.example.com:3478?transport=udp"
        ))),
        vec![
            String::from("stun:one.example.com:3478"),
            String::from("turn:two.example.com:3478?transport=udp"),
        ]
    );
}

#[test]
fn parse_turn_ttl_accepts_positive_values_and_rejects_zero() {
    assert_eq!(
        parse_turn_ttl(Some(String::from("600"))).expect("ttl should parse"),
        Duration::from_secs(600)
    );
    assert_eq!(
        parse_turn_ttl(Some(String::from("0"))).expect_err("zero should be rejected"),
        "RARENA_WEBRTC_TURN_TTL_SECONDS must be greater than zero"
    );
}

#[test]
fn parse_webrtc_config_from_env_uses_defaults_when_variables_are_missing() {
    let previous_stun = std::env::var("RARENA_WEBRTC_STUN_URLS").ok();
    let previous_turn = std::env::var("RARENA_WEBRTC_TURN_URLS").ok();
    let previous_secret = std::env::var("RARENA_WEBRTC_TURN_SECRET").ok();
    let previous_ttl = std::env::var("RARENA_WEBRTC_TURN_TTL_SECONDS").ok();
    std::env::remove_var("RARENA_WEBRTC_STUN_URLS");
    std::env::remove_var("RARENA_WEBRTC_TURN_URLS");
    std::env::remove_var("RARENA_WEBRTC_TURN_SECRET");
    std::env::remove_var("RARENA_WEBRTC_TURN_TTL_SECONDS");

    let result = parse_webrtc_config_from_env().expect("default webrtc config should parse");
    assert_eq!(result, WebRtcRuntimeConfig::default());

    restore_env("RARENA_WEBRTC_STUN_URLS", previous_stun);
    restore_env("RARENA_WEBRTC_TURN_URLS", previous_turn);
    restore_env("RARENA_WEBRTC_TURN_SECRET", previous_secret);
    restore_env("RARENA_WEBRTC_TURN_TTL_SECONDS", previous_ttl);
}

#[test]
fn parse_log_format_from_env_accepts_pretty_and_json_and_rejects_unknown_values() {
    assert_eq!(
        LogFormat::parse("json").expect("json log format should parse"),
        LogFormat::Json
    );
    assert_eq!(
        LogFormat::parse("PRETTY").expect("pretty log format should parse"),
        LogFormat::Pretty
    );
    assert_eq!(
        LogFormat::parse("xml").expect_err("unknown log formats should be rejected"),
        "unsupported RARENA_LOG_FORMAT 'xml'; expected 'pretty' or 'json'"
    );
}

fn restore_env(key: &str, value: Option<String>) {
    if let Some(value) = value {
        std::env::set_var(key, value);
    } else {
        std::env::remove_var(key);
    }
}
