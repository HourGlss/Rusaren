#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

mod cli;
mod client;
mod event_log;
mod planner;
mod probe;

pub use cli::CliArgs;
pub use probe::{run_probe, ProbeConfig, ProbeMechanicObservation, ProbeOutcome};

use std::fmt;

pub type ProbeResult<T> = Result<T, ProbeError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeError {
    message: String,
}

impl ProbeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProbeError {}

impl From<std::io::Error> for ProbeError {
    fn from(value: std::io::Error) -> Self {
        Self::new(value.to_string())
    }
}

impl From<serde_json::Error> for ProbeError {
    fn from(value: serde_json::Error) -> Self {
        Self::new(value.to_string())
    }
}

#[cfg(test)]
mod tests;
