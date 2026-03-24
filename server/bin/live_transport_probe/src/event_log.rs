use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use crate::ProbeResult;

#[derive(Debug)]
pub struct ProbeLogger {
    path: PathBuf,
    writer: BufWriter<File>,
}

#[derive(Serialize)]
struct LogRecord<'a> {
    ts_ms: u128,
    level: &'a str,
    kind: &'a str,
    fields: Value,
}

impl ProbeLogger {
    pub fn new(path: &Path, origin: &str) -> ProbeResult<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(path)?;
        let mut logger = Self {
            path: path.to_path_buf(),
            writer: BufWriter::new(file),
        };
        logger.info("probe_started", serde_json::json!({ "origin": origin }))?;
        Ok(logger)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn info(&mut self, kind: &str, fields: Value) -> ProbeResult<()> {
        self.write("INFO", kind, fields)
    }

    pub fn error(&mut self, kind: &str, fields: Value) -> ProbeResult<()> {
        self.write("ERROR", kind, fields)
    }

    fn write(&mut self, level: &str, kind: &str, fields: Value) -> ProbeResult<()> {
        let record = LogRecord {
            ts_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            level,
            kind,
            fields,
        };
        serde_json::to_writer(&mut self.writer, &record)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        println!("[{level}] {kind}");
        Ok(())
    }
}
