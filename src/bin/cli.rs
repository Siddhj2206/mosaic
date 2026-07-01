use std::io::{self, Write};

use clap::{Parser, Subcommand};
use mosaic_core::db::MosaicDb;
use mosaic_core::types::Ipo;

#[derive(Parser)]
#[command(name = "mosaic-cli", about = "Mosaic IPO tracker CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run scrapers to fetch latest data
    Sync,
    /// List all IPOs in the database
    List {
        /// Filter by status: upcoming, open, closed, listed, withdrawn
        #[arg(long)]
        status: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Sync => cmd_sync(),
        Commands::List { status } => cmd_list(status.as_deref()),
    }
}

fn cmd_sync() -> anyhow::Result<()> {
    let path = mosaic::db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let result = mosaic::run_sync(&path)?;
    println!(
        "{}: {} IPOs, {} updated, {} skipped",
        result.source, result.total, result.updated, result.skipped
    );
    Ok(())
}

fn cmd_list(status_filter: Option<&str>) -> anyhow::Result<()> {
    let path = mosaic::db_path();
    if !path.exists() {
        eprintln!("No database found at {}", path.display());
        eprintln!("Run 'mosaic-cli sync' first to fetch IPO data.");
        return Ok(());
    }

    let db = MosaicDb::open(&path)?;
    let ipos = db.list_ipos(status_filter)?;

    if ipos.is_empty() {
        if status_filter.is_some() {
            println!("No IPOs found with status '{status}'", status = status_filter.unwrap());
        } else {
            println!("No IPOs found. Run 'mosaic-cli sync' first.");
        }
        return Ok(());
    }

    let header = format!(
        "{:<4} {:<28} {:<5} {:<18} {:<11} {:<12} {:<12}",
        "ID", "Company", "Exch", "Price Band", "Status", "Open Date", "Close Date"
    );
    let sep = "-".repeat(header.len());

    println!("{header}");
    println!("{sep}");

    let mut out = io::stdout().lock();
    for ipo in &ipos {
        let id = ipo.id.map(|i| format!("{i}")).unwrap_or_default();
        let exch = ipo.exchange.as_deref().unwrap_or("-");
        let price = fmt_price_band(ipo);
        let status = ipo.status.as_str();
        let open = ipo.open_date.as_deref().unwrap_or("-");
        let close = ipo.close_date.as_deref().unwrap_or("-");

        writeln!(
            out,
            "{id:<4} {:<28} {exch:<5} {price:<18} {status:<11} {open:<12} {close:<12}",
            ipo.company_name,
        )?;
    }

    Ok(())
}

fn fmt_price_band(ipo: &Ipo) -> String {
    match (ipo.price_band_low, ipo.price_band_high) {
        (Some(l), Some(h)) => format!("₹{l} - ₹{h}"),
        (Some(l), None) => format!("₹{l}"),
        _ => "-".into(),
    }
}
