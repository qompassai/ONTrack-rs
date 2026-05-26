//! ontrack — TDS Telecom Field Route Optimizer (CLI)
//!
//! Usage examples:
//!   ontrack route stops.csv
//!   ontrack route stops.csv --open-maps
//!   ontrack route stops.csv --backend osrm --solver 2opt --open
//!   ontrack geocode "123 Main St Spokane WA"
//!   ontrack location
//!
//! Copyright (C) 2025 Qompass AI, All rights reserved

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use ontrack_core::{
    config::Config,
    exporter::{
        build_fieldmaps_url, build_maps_url, build_maps_url_chunked,
        build_waze_url, export_csv, format_route_text,
    },
    geocoder::{geocode_addresses, get_current_location_ip},
    matrix::{build_distance_matrix, MatrixBackend},
    parser::parse_addresses,
    solver::{solve_tsp, SolverBackend},
};
use std::path::PathBuf;
use std::str::FromStr;

// ── CLI definition ─────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name        = "ontrack",
    version     = "2.0.0",
    author      = "Matt Porter <matt@aflabs.io>",
    about       = "TDS Telecom Field Route Optimizer",
    long_about  = "Optimize driving routes for field service technicians.\n\
                   Reads addresses from a CSV file or command-line arguments,\n\
                   geocodes them, computes an optimal drive order, and launches\n\
                   navigation in Google Maps or ArcGIS FieldMaps."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Optimize a driving route from a list of addresses
    Route {
        /// CSV file with an 'address' column, or plain text file (one per line)
        #[arg(value_name = "FILE")]
        file: Option<PathBuf>,

        /// Enter addresses interactively instead of loading from a file
        #[arg(short, long)]
        interactive: bool,

        /// Distance/duration matrix backend: osrm (default), google, haversine
        #[arg(long, default_value = "osrm", env = "ONTRACK_BACKEND")]
        backend: String,

        /// Solver algorithm: 2opt (default), nn
        #[arg(long, default_value = "2opt", env = "ONTRACK_SOLVER")]
        solver: String,

        /// Starting stop index (0 = first address in list)
        #[arg(long, default_value = "0")]
        depot: usize,

        /// Open route in Google Maps after solving
        #[arg(long)]
        open_maps: bool,

        /// Open first stop in ArcGIS FieldMaps after solving
        #[arg(long)]
        open_fieldmaps: bool,

        /// Open first stop in Waze after solving
        #[arg(long)]
        open_waze: bool,

        /// Export route to CSV file
        #[arg(long, value_name = "OUTPUT_CSV")]
        export: Option<PathBuf>,

        /// Open route (driver does not return to start)
        #[arg(long, default_value = "true")]
        open_route: bool,
    },

    /// Geocode one or more addresses and print their lat/lng
    Geocode {
        /// Address strings to geocode
        #[arg(required = true, num_args = 1..)]
        addresses: Vec<String>,
    },

    /// Show the current device location via IP geolocation
    Location,

    /// Print a summary of the current configuration (.env values)
    Config,
}

// ── Entry point ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Init logging
    let level = if cli.verbose { "debug" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .init();

    let config = Config::from_env();
    let http = reqwest::Client::builder()
        .user_agent("OnTrack-TDS/2.0")
        .build()?;

    match cli.command {
        Commands::Route {
            file,
            interactive,
            backend,
            solver,
            depot,
            open_maps,
            open_fieldmaps,
            open_waze,
            export,
            open_route,
        } => {
            run_route(
                &http,
                &config,
                file,
                interactive,
                &backend,
                &solver,
                depot,
                open_maps,
                open_fieldmaps,
                open_waze,
                export,
                open_route,
            )
            .await?
        }
        Commands::Geocode { addresses } => run_geocode(&http, &config, &addresses).await?,
        Commands::Location => run_location(&http).await?,
        Commands::Config => run_config(&config),
    }

    Ok(())
}

// ── Subcommand handlers ────────────────────────────────────────────────────

async fn run_route(
    http: &reqwest::Client,
    config: &Config,
    file: Option<PathBuf>,
    interactive: bool,
    backend_str: &str,
    solver_str: &str,
    depot: usize,
    open_maps: bool,
    open_fieldmaps: bool,
    open_waze: bool,
    export: Option<PathBuf>,
    open_route: bool,
) -> Result<()> {
    // ── Step 1: collect addresses ──
    let addresses = if interactive {
        read_addresses_interactive()?
    } else if let Some(path) = file {
        println!("{}", "Loading addresses…".dimmed());
        parse_addresses(&path)?
    } else {
        anyhow::bail!(
            "Provide a FILE argument or use --interactive.\n\
             Example: ontrack route stops.csv"
        );
    };

    if addresses.len() < 2 {
        anyhow::bail!("Need at least 2 addresses to optimize a route.");
    }

    println!(
        "{} {} addresses loaded",
        "✓".green().bold(),
        addresses.len()
    );

    // ── Step 2: geocode ──
    let pb = spinner("Geocoding addresses…");
    let mut geocoded_count = 0usize;
    let locations = geocode_addresses(
        http,
        &addresses,
        config.has_google_key() && backend_str == "google",
        config.google_maps_api_key.as_deref(),
        |done, total| {
            pb.set_message(format!("Geocoding {done}/{total}…"));
            geocoded_count = done;
        },
    )
    .await;
    pb.finish_and_clear();

    let failed: Vec<_> = locations.iter().filter(|l| !l.is_resolved()).collect();
    if !failed.is_empty() {
        println!(
            "{} {} address(es) could not be geocoded and will be skipped:",
            "⚠".yellow(),
            failed.len()
        );
        for f in &failed {
            println!("    {}", f.address.yellow());
        }
    }

    // ── Step 3: distance matrix ──
    let matrix_backend = MatrixBackend::from_str(backend_str)?;
    let pb = spinner("Building distance matrix…");
    let (resolved, matrix) = build_distance_matrix(
        http,
        &locations,
        &matrix_backend,
        &config.osrm_base_url,
        config.google_maps_api_key.as_deref(),
    )
    .await?;
    pb.finish_and_clear();

    println!(
        "{} {} stops after geocoding",
        "✓".green().bold(),
        resolved.len()
    );

    // ── Step 4: solve ──
    let solver_backend = SolverBackend::from_str(solver_str)?;
    let depot_idx = depot.min(resolved.len().saturating_sub(1));

    let pb = spinner("Optimizing route…");
    let result = solve_tsp(&resolved, &matrix, depot_idx, &solver_backend, open_route)?;
    pb.finish_and_clear();

    // ── Step 5: print results ──
    println!();
    println!(
        "{}  {} stops  ·  {}  ·  solver: {:?}  ·  matrix: {}",
        "✓ Route ready".green().bold(),
        result.ordered_addresses.len(),
        result.format_duration(),
        result.backend_used,
        backend_str,
    );
    println!();

    for (i, addr) in result.ordered_addresses.iter().enumerate() {
        println!("  {}  {}", format!("{:>2}.", i + 1).bright_yellow(), addr);
    }
    println!();

    // Maps URL
    let maps_url = if result.ordered_addresses.len() <= 10 {
        vec![build_maps_url(&result.ordered_addresses)]
    } else {
        build_maps_url_chunked(&result.ordered_addresses)
    };
    println!("{}", "Google Maps:".bold());
    for url in &maps_url {
        println!("  {}", url.blue().underline());
    }

    // FieldMaps URL for first stop
    if let Some(first) = resolved.first() {
        let fm_url = build_fieldmaps_url(
            &first.address,
            first.lat,
            first.lng,
            config.arcgis_item_id.as_deref(),
            2000,
        );
        println!("{}", "ArcGIS FieldMaps:".bold());
        println!("  {}", fm_url.blue().underline());
    }

    // ── Step 6: export ──
    if let Some(out_path) = export {
        export_csv(&result.ordered_addresses, &out_path)?;
        println!("{} Route exported to {}", "✓".green(), out_path.display());
    }

    // ── Step 7: open in apps ──
    if open_maps {
        for url in &maps_url {
            open::that(url)?;
        }
    }

    if open_fieldmaps {
        if let Some(first) = resolved.first() {
            let url = build_fieldmaps_url(
                &first.address,
                first.lat,
                first.lng,
                config.arcgis_item_id.as_deref(),
                2000,
            );
            open::that(url)?;
        }
    }

    if open_waze {
        if let Some(first) = resolved.first() {
            if let (Some(lat), Some(lng)) = (first.lat, first.lng) {
                open::that(build_waze_url(lat, lng))?;
            }
        }
    }

    Ok(())
}

async fn run_geocode(
    http: &reqwest::Client,
    config: &Config,
    addresses: &[String],
) -> Result<()> {
    let pb = spinner("Geocoding…");
    let locations = geocode_addresses(
        http,
        addresses,
        config.has_google_key(),
        config.google_maps_api_key.as_deref(),
        |done, total| pb.set_message(format!("{done}/{total}")),
    )
    .await;
    pb.finish_and_clear();

    for loc in &locations {
        match (loc.lat, loc.lng) {
            (Some(lat), Some(lng)) => println!(
                "{}  {:>10.6}, {:>11.6}  —  {}",
                "✓".green(),
                lat,
                lng,
                loc.address
            ),
            _ => println!("{}  (not found)  —  {}", "✗".red(), loc.address),
        }
    }
    Ok(())
}

async fn run_location(http: &reqwest::Client) -> Result<()> {
    let pb = spinner("Detecting location…");
    let loc = get_current_location_ip(http).await;
    pb.finish_and_clear();
    match loc {
        Some(l) => println!(
            "{} lat {:.5}, lng {:.5}  (IP geolocation — coarse)",
            "📍".to_string(),
            l.lat.unwrap(),
            l.lng.unwrap()
        ),
        None => println!("{} Could not determine location.", "✗".red()),
    }
    Ok(())
}

fn run_config(config: &Config) {
    println!("{}", "OnTrack Configuration".bold());
    println!(
        "  GOOGLE_MAPS_API_KEY  {}",
        if config.has_google_key() {
            "set ✓".green().to_string()
        } else {
            "not set (using Nominatim + OSRM free)".yellow().to_string()
        }
    );
    println!("  OSRM_BASE_URL        {}", config.osrm_base_url);
    println!(
        "  ARCGIS_ITEM_ID       {}",
        config
            .arcgis_item_id
            .as_deref()
            .unwrap_or("not set")
    );
    println!("  Version              {}", config.app_version);
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✓"]),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

fn read_addresses_interactive() -> Result<Vec<String>> {
    use std::io::{self, BufRead, Write};
    println!("{}", "Enter addresses (one per line, empty line to finish):".bold());
    let stdin = io::stdin();
    let mut addresses = Vec::new();
    loop {
        print!("  > ");
        io::stdout().flush()?;
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let line = line.trim().to_string();
        if line.is_empty() {
            break;
        }
        addresses.push(line);
    }
    Ok(addresses)
}
