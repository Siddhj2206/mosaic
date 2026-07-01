use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mosaic-cli", about = "Mosaic IPO tracker CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all IPOs in the database
    List,
    /// Run scrapers to fetch latest data
    Sync,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::List => {
            println!("IPOs: (not yet implemented)");
        }
        Commands::Sync => {
            println!("Syncing... (not yet implemented)");
        }
    }
    Ok(())
}
