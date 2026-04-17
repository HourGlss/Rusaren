use std::env;
use std::path::PathBuf;
use std::time::Duration;

use crate::{ProbeConfig, ProbeError, ProbeResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliArgs {
    pub origin: String,
    pub output: PathBuf,
    pub max_games: Option<usize>,
    pub max_rounds: Option<usize>,
}

impl CliArgs {
    pub fn parse_from_env() -> ProbeResult<Self> {
        Self::parse_from_iter(env::args().skip(1))
    }

    fn parse_from_iter(arguments: impl Iterator<Item = String>) -> ProbeResult<Self> {
        let mut origin = None;
        let mut output = None;
        let mut max_games = None;
        let mut max_rounds = None;

        let mut args = arguments;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--origin" => {
                    origin = Some(Self::parse_string_flag(&mut args, "--origin")?);
                }
                "--output" => {
                    output = Some(PathBuf::from(Self::parse_string_flag(
                        &mut args, "--output",
                    )?));
                }
                "--max-games" => {
                    max_games = Some(Self::parse_usize_flag(&mut args, "--max-games")?);
                }
                "--max-rounds" => {
                    max_rounds = Some(Self::parse_usize_flag(&mut args, "--max-rounds")?);
                }
                "--help" | "-h" => {
                    return Err(ProbeError::new(Self::usage()));
                }
                other => {
                    return Err(ProbeError::new(format!(
                        "unrecognized argument {other:?}\n\n{}",
                        Self::usage()
                    )));
                }
            }
        }

        let origin = origin.ok_or_else(|| {
            ProbeError::new(format!("missing required --origin\n\n{}", Self::usage()))
        })?;
        let output = output.ok_or_else(|| {
            ProbeError::new(format!("missing required --output\n\n{}", Self::usage()))
        })?;

        Ok(Self {
            origin,
            output,
            max_games,
            max_rounds,
        })
    }

    fn parse_string_flag(
        args: &mut impl Iterator<Item = String>,
        flag: &str,
    ) -> ProbeResult<String> {
        args.next()
            .ok_or_else(|| ProbeError::new(format!("expected a value after {flag}")))
    }

    fn parse_usize_flag(args: &mut impl Iterator<Item = String>, flag: &str) -> ProbeResult<usize> {
        let raw = Self::parse_string_flag(args, flag)?;
        raw.parse::<usize>()
            .map_err(|error| ProbeError::new(format!("invalid {flag} value {raw:?}: {error}")))
    }

    #[must_use]
    pub fn into_probe_config(self) -> ProbeConfig {
        ProbeConfig {
            origin: self.origin,
            output_path: self.output,
            content_root: None,
            max_games: self.max_games,
            connect_timeout: Duration::from_secs(20),
            stage_timeout: Duration::from_secs(30),
            round_timeout: Duration::from_secs(90),
            match_timeout: Duration::from_secs(600),
            input_cadence: Duration::from_millis(100),
            players_per_match: 4,
            preferred_tree_order: None,
            max_rounds_per_match: self.max_rounds,
            max_combat_loops_per_round: None,
            required_mechanics: None,
        }
    }

    fn usage() -> &'static str {
        "usage: cargo run -p live_transport_probe -- --origin https://host --output /path/to/log.jsonl [--max-games N] [--max-rounds N]"
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::CliArgs;

    fn parse(arguments: &[&str]) -> Result<CliArgs, String> {
        CliArgs::parse_from_iter(arguments.iter().map(|argument| String::from(*argument)))
            .map_err(|error| error.to_string())
    }

    #[test]
    fn parse_from_iter_accepts_required_and_optional_flags() {
        let args = parse(&[
            "--origin",
            "https://example.test",
            "--output",
            "probe.jsonl",
            "--max-games",
            "3",
            "--max-rounds",
            "5",
        ])
        .expect("arguments should parse");

        assert_eq!(args.origin, "https://example.test");
        assert_eq!(args.output, PathBuf::from("probe.jsonl"));
        assert_eq!(args.max_games, Some(3));
        assert_eq!(args.max_rounds, Some(5));
    }

    #[test]
    fn parse_from_iter_requires_origin() {
        let error = parse(&["--output", "probe.jsonl"]).expect_err("origin is required");
        assert!(error.contains("missing required --origin"));
    }

    #[test]
    fn parse_from_iter_requires_output() {
        let error = parse(&["--origin", "https://example.test"]).expect_err("output is required");
        assert!(error.contains("missing required --output"));
    }

    #[test]
    fn parse_from_iter_rejects_invalid_numeric_flags() {
        let error = parse(&[
            "--origin",
            "https://example.test",
            "--output",
            "probe.jsonl",
            "--max-games",
            "abc",
        ])
        .expect_err("invalid max-games should fail");
        assert!(error.contains("invalid --max-games value"));
    }

    #[test]
    fn parse_from_iter_rejects_unknown_flags() {
        let error = parse(&[
            "--origin",
            "https://example.test",
            "--output",
            "probe.jsonl",
            "--bogus",
        ])
        .expect_err("unknown flags should fail");
        assert!(error.contains("unrecognized argument"));
    }

    #[test]
    fn into_probe_config_preserves_cli_values() {
        let config = parse(&[
            "--origin",
            "https://example.test",
            "--output",
            "probe.jsonl",
            "--max-games",
            "2",
            "--max-rounds",
            "4",
        ])
        .expect("arguments should parse")
        .into_probe_config();

        assert_eq!(config.origin, "https://example.test");
        assert_eq!(config.output_path, PathBuf::from("probe.jsonl"));
        assert_eq!(config.max_games, Some(2));
        assert_eq!(config.max_rounds_per_match, Some(4));
        assert_eq!(config.round_timeout, Duration::from_secs(90));
        assert_eq!(config.match_timeout, Duration::from_secs(600));
        assert!(config.content_root.is_none());
    }
}
