use clap::{Parser, Subcommand};
use cmt::{get, query, watch};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version)]
/// crimson metrics tool
struct CMT {
    #[command(subcommand)]
    commands: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// observe the metric
    Get {
        /// admin socket path
        path: PathBuf,
        /// metric name or regex
        pattern: String,
    },
    /// watch and store metrics
    Watch {
        /// admin socket lists
        paths: Vec<PathBuf>,
        /// intervals of sampling
        #[arg(short, long, default_value_t = 15)]
        interval: u64,
    },
    /// query and plot metrics
    Query {
        /// data file produeced by watch command
        file: PathBuf,
    },
}

pub fn main() {
    let cmt = CMT::parse();
    if let Err(e) = match &cmt.commands {
        Commands::Get { path, pattern } => get(path, pattern),
        Commands::Watch { paths, interval } => watch(paths, *interval),
        Commands::Query { file } => query(file),
    } {
        eprintln!("error: {}", e);
    }
}
