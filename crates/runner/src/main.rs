use runner::{BenchmarkPipeline, PipelineError};
use std::env;
use std::path::Path;
use std::process;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

fn main() {
    // Initialize structured logging
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args: Vec<String> = env::args().collect();

    // Check for help flag
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage(&args[0]);
        return;
    }

    let json_output = args.iter().any(|a| a == "--json");
    let config_path = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "configs/sample.toml".to_string());

    info!(config = %config_path, "Running database benchmark pipeline");

    if !Path::new(&config_path).exists() {
        eprintln!("Error: configuration file '{config_path}' does not exist.");
        eprintln!("Usage: {} [CONFIG_PATH] [--json]", args[0]);
        process::exit(1);
    }

    match BenchmarkPipeline::run_from_file(&config_path) {
        Ok(result) => {
            if json_output {
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => println!("{json}"),
                    Err(e) => {
                        error!("Failed to serialize result to JSON: {e}");
                        println!("{}", result.format_summary());
                    }
                }
            } else {
                println!("{}", result.format_summary());
            }

            if result.has_recommendation() {
                process::exit(0);
            } else {
                // Completed, but no candidates were eligible
                process::exit(2);
            }
        }
        Err(err) => {
            eprintln!("\nBenchmark pipeline execution failed:");
            match &err {
                PipelineError::ConfigNotFound(path) => {
                    eprintln!("  Config not found: {path}");
                }
                PipelineError::ConfigParse(msg) => {
                    eprintln!("  Config parse error: {msg}");
                }
                PipelineError::InvalidConfig(msg) => {
                    eprintln!("  Invalid configuration: {msg}");
                }
                PipelineError::Dataset(msg) => {
                    eprintln!("  Dataset error: {msg}");
                }
                PipelineError::Benchmark(msg) => {
                    eprintln!("  Benchmark error: {msg}");
                }
                PipelineError::Evaluation(msg) => {
                    eprintln!("  Evaluation error: {msg}");
                }
                PipelineError::Recommendation(msg) => {
                    eprintln!("  Recommendation error: {msg}");
                }
                PipelineError::Environment(msg) => {
                    eprintln!("  Environment error: {msg}");
                }
                PipelineError::Other(e) => {
                    eprintln!("  Unexpected error: {e}");
                }
            }
            process::exit(1);
        }
    }
}

fn print_usage(bin_name: &str) {
    println!("Database Benchmarking and Recommendation Tool");
    println!();
    println!("USAGE:");
    println!("    {bin_name} [CONFIG_PATH] [FLAGS]");
    println!();
    println!("ARGS:");
    println!("    <CONFIG_PATH>    Path to benchmark TOML config [default: configs/sample.toml]");
    println!();
    println!("FLAGS:");
    println!("    --json           Output pipeline result as JSON");
    println!("    -h, --help       Print help information");
}
