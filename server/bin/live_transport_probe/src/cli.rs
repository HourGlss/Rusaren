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
        let mut origin = None;
        let mut output = None;
        let mut max_games = None;
        let mut max_rounds = None;

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--origin" => {
                    origin = Some(
                        args.next()
                            .ok_or_else(|| ProbeError::new("expected a value after --origin"))?,
                    );
                }
                "--output" => {
                    output =
                        Some(PathBuf::from(args.next().ok_or_else(|| {
                            ProbeError::new("expected a value after --output")
                        })?));
                }
                "--max-games" => {
                    let raw = args
                        .next()
                        .ok_or_else(|| ProbeError::new("expected a value after --max-games"))?;
                    max_games = Some(raw.parse::<usize>().map_err(|error| {
                        ProbeError::new(format!("invalid --max-games value {raw:?}: {error}"))
                    })?);
                }
                "--max-rounds" => {
                    let raw = args
                        .next()
                        .ok_or_else(|| ProbeError::new("expected a value after --max-rounds"))?;
                    max_rounds = Some(raw.parse::<usize>().map_err(|error| {
                        ProbeError::new(format!("invalid --max-rounds value {raw:?}: {error}"))
                    })?);
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

    #[must_use]
    pub fn into_probe_config(self) -> ProbeConfig {
        ProbeConfig {
            origin: self.origin,
            output_path: self.output,
            max_games: self.max_games,
            connect_timeout: Duration::from_secs(20),
            stage_timeout: Duration::from_secs(30),
            round_timeout: Duration::from_secs(45),
            match_timeout: Duration::from_secs(300),
            input_cadence: Duration::from_millis(100),
            players_per_match: 4,
            preferred_tree_order: None,
            max_rounds_per_match: self.max_rounds,
            max_combat_loops_per_round: None,
        }
    }

    fn usage() -> &'static str {
        "usage: cargo run -p live_transport_probe -- --origin https://host --output /path/to/log.jsonl [--max-games N] [--max-rounds N]"
    }
}
