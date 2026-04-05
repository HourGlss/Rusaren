use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use game_content::{generate_template_match_map, render_ascii_map, GameContent};

const USAGE: &str =
    "usage: cargo run -p map_sample_builder -- [--count 100] [--output-dir PATH]";

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse(std::env::args_os().skip(1))?;
    let content = GameContent::bundled()?;
    let template = content
        .map_by_id("template_arena")
        .ok_or_else(|| String::from("bundled content is missing template_arena"))?;

    fs::create_dir_all(&args.output_dir)?;
    remove_existing_samples(&args.output_dir)?;

    for index in 1..=args.count {
        let file_stem = format!("sample_{index:03}");
        let seed = splitmix64(u64::from(index));
        let map = generate_template_match_map(
            template,
            &content.configuration().maps.generation,
            &file_stem,
            seed,
        )?;
        let ascii = render_ascii_map(&map)?;
        fs::write(
            args.output_dir.join(format!("{file_stem}.txt")),
            format!("{ascii}\n"),
        )?;
    }

    println!(
        "generated {} sample maps in {}",
        args.count,
        args.output_dir.display()
    );
    Ok(())
}

#[derive(Debug)]
struct Args {
    count: u32,
    output_dir: PathBuf,
}

impl Args {
    fn parse(arguments: impl Iterator<Item = OsString>) -> Result<Self, Box<dyn Error>> {
        let mut count = 100_u32;
        let mut output_dir = default_output_dir();
        let mut args = arguments;
        while let Some(argument) = args.next() {
            match argument.to_string_lossy().as_ref() {
                "--count" => {
                    let Some(value) = args.next() else {
                        return Err(String::from("missing value after --count").into());
                    };
                    count = value
                        .to_string_lossy()
                        .parse::<u32>()
                        .map_err(|error| format!("invalid --count value: {error}"))?;
                }
                "--output-dir" => {
                    let Some(value) = args.next() else {
                        return Err(String::from("missing value after --output-dir").into());
                    };
                    output_dir = PathBuf::from(value);
                }
                "--help" | "-h" => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                other => {
                    return Err(format!("unrecognized argument: {other}").into());
                }
            }
        }

        Ok(Self { count, output_dir })
    }
}

fn default_output_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content")
        .join("maps")
        .join("generated")
}

fn remove_existing_samples(output_dir: &Path) -> Result<(), Box<dyn Error>> {
    if !output_dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(output_dir)? {
        let path = entry?.path();
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if file_name.starts_with("sample_")
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
        {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut result = value;
    result = (result ^ (result >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    result = (result ^ (result >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    result ^ (result >> 31)
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{default_output_dir, remove_existing_samples, splitmix64, Args};

    fn parse(arguments: &[&str]) -> Result<Args, Box<dyn Error>> {
        Args::parse(arguments.iter().map(OsString::from))
    }

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("rarena-map-sample-builder-{nanos}"))
    }

    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn parse_uses_defaults() {
        let args = parse(&[]).expect("default arguments should parse");
        assert_eq!(args.count, 100);
        assert_eq!(args.output_dir, default_output_dir());
    }

    #[test]
    fn parse_accepts_overrides() {
        let args = parse(&["--count", "12", "--output-dir", "custom-output"])
            .expect("override arguments should parse");
        assert_eq!(args.count, 12);
        assert_eq!(args.output_dir, PathBuf::from("custom-output"));
    }

    #[test]
    fn parse_rejects_unknown_arguments() {
        let error = parse(&["--bogus"]).expect_err("unknown flags should fail");
        assert!(error.to_string().contains("unrecognized argument"));
    }

    #[test]
    fn parse_rejects_invalid_counts() {
        let error = parse(&["--count", "abc"]).expect_err("invalid count should fail");
        assert!(error.to_string().contains("invalid --count value"));
    }

    #[test]
    fn remove_existing_samples_only_deletes_generated_text_files() {
        let output_dir = unique_temp_dir();
        fs::create_dir_all(&output_dir).expect("temp directory should be created");
        fs::write(output_dir.join("sample_001.txt"), "sample").expect("sample file should exist");
        fs::write(output_dir.join("sample_001.json"), "{}").expect("json file should exist");
        fs::write(output_dir.join("notes.txt"), "keep").expect("notes file should exist");

        remove_existing_samples(&output_dir).expect("cleanup should succeed");

        assert!(!output_dir.join("sample_001.txt").exists());
        assert!(output_dir.join("sample_001.json").exists());
        assert!(output_dir.join("notes.txt").exists());

        fs::remove_dir_all(&output_dir).expect("temp directory should be removable");
    }

    #[test]
    fn splitmix64_is_deterministic() {
        assert_eq!(splitmix64(7), splitmix64(7));
        assert_ne!(splitmix64(7), splitmix64(8));
    }
}
