//! Luminair Infrastructure CLI binary entry point.

use std::process::ExitCode;

use infrastructure::cli::{CliConfig, CliError};

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    match run_cli().await {
        Ok(rendered) => {
            println!("{rendered}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}

async fn run_cli() -> Result<String, CliError> {
    let config = CliConfig::from_env_and_args()?;
    let outcome = infrastructure::cli::run(&config).await?;
    outcome.write_report(config.output_format, config.output_path.as_deref())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}
