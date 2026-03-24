use live_transport_probe::{run_probe, CliArgs};

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = CliArgs::parse_from_env()?;
    let config = args.into_probe_config();
    let outcome = run_probe(config).await?;
    println!(
        "live transport probe passed: matches={} covered_skills={}/{} log={}",
        outcome.matches_completed,
        outcome.covered_skills,
        outcome.total_skills,
        outcome.log_path.display()
    );
    Ok(())
}
