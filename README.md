# ontrack-rs — TDS Telecom Field Route Optimizer (Rust)

Rust implementation of [OnTrack](../ontrack/README.md) — same features, faster binary, no Python runtime required.

---

## Crates

| Crate | Binary | Description |
|---|---|---|
| `ontrack-core` | library | Geocoding, distance matrix, TSP solver, exporter — shared logic |
| `ontrack-cli`  | `ontrack` | Full-featured CLI with progress bars and coloured output |
| `ontrack-gui`  | `ontrack-gui` | egui desktop GUI — Windows, Linux, macOS |

---

## Quick Start

```bash
cd ontrack-rs

# Build everything
cargo build --release

# Run CLI
./target/release/ontrack route stops.csv --open-maps

# Run GUI
./target/release/ontrack-gui
```

---

## CLI Usage

```
ontrack [OPTIONS] <COMMAND>

Commands:
  route     Optimize a driving route from a list of addresses
  geocode   Geocode one or more addresses and print their lat/lng
  location  Show the current device location via IP geolocation
  config    Print current configuration (.env values)

Options:
  -v, --verbose   Enable verbose logging
  -h, --help      Print help
  -V, --version   Print version
```

### Route command

```bash
# Load addresses from CSV, optimize, print route
ontrack route stops.csv

# Open result in Google Maps
ontrack route stops.csv --open-maps

# Open first stop in ArcGIS FieldMaps
ontrack route stops.csv --open-fieldmaps

# Use Google distance matrix (requires GOOGLE_MAPS_API_KEY)
ontrack route stops.csv --backend google

# Export to CSV
ontrack route stops.csv --export route_output.csv

# Round trip (return to start)
ontrack route stops.csv --open-route false

# Enter addresses interactively
ontrack route --interactive
```

### Address file format

CSV with an `address` column:
```csv
address
123 Main St Spokane WA
456 Elm St Coeur d'Alene ID
789 Oak Ave Post Falls ID
```

Or plain text (one address per line):
```
123 Main St Spokane WA
456 Elm St Coeur d'Alene ID
# lines starting with # are comments
```

---

## Configuration

Copy `.env.example` (in `../ontrack/`) to `ontrack-rs/.env`:

```bash
cp ../ontrack/.env.example .env
```

```env
GOOGLE_MAPS_API_KEY=""   # optional — enables Street View + Google routing
OSRM_BASE_URL="http://router.project-osrm.org"
ARCGIS_ITEM_ID=""        # optional — your ArcGIS Online web map ID
```

**No API key required.** Without a key, the app uses:
- Nominatim (OpenStreetMap) for geocoding
- OSRM public router for distances
- OSM tiles for map previews
- Plain Google Maps URL scheme for navigation

---

## Solver

The Rust solver uses **Nearest-Neighbour + 2-opt local search** — pure Rust, no C++ FFI, no OR-Tools dependency. For typical field routes (≤ 50 stops) this produces near-optimal results comparable to OR-Tools with a 30-second time limit.

| Backend | Algorithm | Quality | Speed |
|---|---|---|---|
| `2opt` (default) | NN seed + 2-opt local search | Near-optimal | < 1s for 50 stops |
| `nn` | Greedy nearest-neighbour | Good | < 10ms for 50 stops |

---

## Build — Production Binaries

### Linux x86_64
```bash
cargo build --release
# Binaries: target/release/ontrack  target/release/ontrack-gui
```

### Windows (cross-compile from Linux)
```bash
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

### Windows (native)
```powershell
cargo build --release
# Binaries: target\release\ontrack.exe  target\release\ontrack-gui.exe
```

### Static binary (fully self-contained, no glibc)
```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

---

## Tests

```bash
# Run all tests
cargo test

# Run tests for core library only
cargo test -p ontrack-core

# With output
cargo test -- --nocapture
```

---

## Architecture

```
ontrack-rs/
├── Cargo.toml                  # workspace
├── ontrack-core/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── config.rs           # .env / environment loading
│       ├── parser.rs           # CSV / plain-text address files
│       ├── geocoder.rs         # Nominatim + Google geocoding
│       ├── matrix.rs           # haversine + OSRM + Google distance matrix
│       ├── solver.rs           # nearest-neighbour + 2-opt TSP
│       └── exporter.rs         # CSV, Maps URL, FieldMaps, Street View, Waze
├── ontrack-cli/
│   ├── Cargo.toml
│   └── src/main.rs             # clap CLI with indicatif progress bars
└── ontrack-gui/
    ├── Cargo.toml
    └── src/main.rs             # egui three-panel desktop GUI
```
