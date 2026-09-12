# A Web Scraper Written in Rust

Rust-based async scraper for collecting structured aircraft data from PlanePhD.

The scraper runs in three pipeline stages:

1. Collect manufacturer links.
2. Collect aircraft/model detail links for every manufacturer.
3. Collect detailed aircraft specifications, performance data, ownership costs, engine data, and image metadata.

## Project Status

This project is being resurrected and modernized.

Current focus areas:

* Rust 2024 compatibility
* Clap 4 CLI migration
* structured error handling with `thiserror`
* reusable HTTP fetching through `Fetcher`
* retry/backoff handling for `429 Too Many Requests`
* JSON persistence through `io.rs`
* staged scraping pipeline using `manufacturers.json`, `aircraft.json`, and `planes.json`

## Requirements

* Rust `1.96.0`
* Cargo
* Network access to PlanePhD
* Optional environment variable: `ACCESS_TOKEN`

Check the toolchain:

```bash
rustc --version
cargo --version
```

## Install / Build

```bash
cargo build
```

Run validation:

```bash
cargo fmt
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## CLI Overview

List available spiders:

```bash
cargo run -- spiders
```

Expected output:

```text
m - ManufacturersSpider
mod - ModelsSpider
p - PlanesSpider
d - Debug directory listing
```

Run a spider:

```bash
cargo run -- run --spider <SPIDER>
```

Valid spider names:

| Spider | Purpose                                           |
| ------ | ------------------------------------------------- |
| `m`    | Scrape manufacturers                              |
| `mod`  | Scrape aircraft/model links for each manufacturer |
| `p`    | Scrape detailed aircraft pages                    |
| `d`    | Debug directory listing                           |

## CLI Configuration

The `run` command supports crawler and file-path configuration:

```bash
cargo run -- run \
  --spider mod \
  --delay-ms 1000 \
  --crawl-concurrency 1 \
  --process-concurrency 32 \
  --manufacturers-file data/manufacturers.json \
  --aircraft-file data/aircraft.json \
  --planes-file data/planes.json
```

### Options

| Option                  |                   Default | Purpose                                        |
| ----------------------- | ------------------------: | ---------------------------------------------- |
| `--spider`              |                  required | Spider to run: `m`, `mod`, `p`, or `d`         |
| `--delay-ms`            |                    `1000` | Delay between crawl operations in milliseconds |
| `--crawl-concurrency`   |                       `1` | Number of concurrent page fetchers             |
| `--process-concurrency` |                      `32` | Number of concurrent item processors           |
| `--manufacturers-file`  | `data/manufacturers.json` | Manufacturer output/input file                 |
| `--aircraft-file`       |      `data/aircraft.json` | Aircraft/model output/input file               |
| `--planes-file`         |        `data/planes.json` | Full aircraft detail output file               |

## Data Pipeline

Run the pipeline in order.

### 1. Scrape Manufacturers

```bash
cargo run -- run \
  --spider m \
  --delay-ms 2000 \
  --crawl-concurrency 1 \
  --process-concurrency 4 \
  --manufacturers-file data/manufacturers.json
```

Output:

```text
data/manufacturers.json
```

Example shape:

```json
{
  "AERO COMMANDER": "/wizard/manufacturers/AERO-COMMANDER/",
  "CESSNA": "/wizard/manufacturers/CESSNA/",
  "PIPER": "/wizard/manufacturers/PIPER/"
}
```

### 2. Scrape Aircraft / Model Links

```bash
cargo run -- run \
  --spider mod \
  --delay-ms 3000 \
  --crawl-concurrency 1 \
  --process-concurrency 4 \
  --manufacturers-file data/manufacturers.json \
  --aircraft-file data/aircraft.json
```

Input:

```text
data/manufacturers.json
```

Output:

```text
data/aircraft.json
```

Example shape:

```json
{
  "AERO COMMANDER": {
    "100 (1960 - 1967)": "/wizard/details/1246/AERO-COMMANDER-100-specifications-performance-operating-cost-valuation",
    "500S (1951 - 1986)": "/wizard/details/1248/AERO-COMMANDER--Twin-Commander--500S-specifications-performance-operating-cost-valuation"
  },
  "CESSNA": {
    "120 (1946 - 1949)": "/wizard/details/146/CESSNA-120-specifications-performance-operating-cost-valuation"
  }
}
```

### 3. Scrape Full Aircraft Details

```bash
cargo run -- run \
  --spider p \
  --delay-ms 3000 \
  --crawl-concurrency 1 \
  --process-concurrency 2 \
  --aircraft-file data/aircraft.json \
  --planes-file data/planes.json
```

Input:

```text
data/aircraft.json
```

Output:

```text
data/planes.json
```

Example shape:

```json
{
  "CESSNA": {
    "120 (1946 - 1949)": {
      "manufacturer_name": "CESSNA",
      "aircraft_name": "120 (1946 - 1949)",
      "source_link": "/wizard/details/146/CESSNA-120-specifications-performance-operating-cost-valuation",
      "page_url": "https://planephd.com/wizard/details/146/CESSNA-120-specifications-performance-operating-cost-valuation",
      "title": "1946 CESSNA 120",
      "description": "Single engine piston aircraft with fixed landing gear. The 120 seats up to 1 passengers plus 1 pilot.",
      "papi_price_estimate": "$39,637",
      "for_sale_count": "4",
      "performance": {
        "horsepower": "1 x 85 HP",
        "best_cruise_speed": "100 KIAS",
        "best_range_i": "390 NM",
        "fuel_burn_75": "4.8 GPH"
      },
      "weights": {
        "gross_weight": "1,450 LBS",
        "empty_weight": "818 LBS",
        "fuel_capacity": "25 GAL"
      },
      "ownership_costs": {
        "total_cost_of_ownership": "$16,747.20",
        "total_fixed_cost": "$4,969.30"
      },
      "engine": {
        "manufacturer": "Cont Motor",
        "model": "C-85-12",
        "horsepower": "85 HP",
        "overhaul_ht": "1,800 Hrs"
      },
      "images": [
        {
          "href": "/static/acftref/2071402_1.jpg",
          "title": "Planephd credits this picture...",
          "holder": "Les Rickman",
          "dimensions": "800x600"
        }
      ]
    }
  }
}
```

## Recommended Safe Crawl Settings

PlanePhD may return `429 Too Many Requests` when requests arrive too quickly.

Use conservative settings while developing:

```bash
--delay-ms 3000
--crawl-concurrency 1
--process-concurrency 2
```

For manufacturer/model scraping:

```bash
--delay-ms 3000
--crawl-concurrency 1
--process-concurrency 4
```

For aircraft detail scraping:

```bash
--delay-ms 3000
--crawl-concurrency 1
--process-concurrency 2
```

## HTTP Fetching

HTTP behavior is centralized in `fetcher.rs`.

Responsibilities:

* construct a reusable `reqwest::Client`
* configure request headers
* use a configured user agent
* attach optional bearer token from `ACCESS_TOKEN`
* handle request timeouts
* retry transient request failures
* retry `429 Too Many Requests`
* retry `5xx` responses
* cap retry delay to avoid apparent hangs

Environment variables:

| Variable       | Purpose                                                       |
| -------------- | ------------------------------------------------------------- |
| `ACCESS_TOKEN` | Optional bearer token used as `Authorization: Bearer <token>` |

## Error Handling

Project errors are defined in `error.rs` through `thiserror`.

Expected error variants include:

```rust
#[error("Request failed: {0}")]
Reqwest(#[from] reqwest::Error),

#[error("HTTP status error: {status} for {url}")]
HttpStatus {
  url: String,
  status: reqwest::StatusCode,
},

#[error("Rate limited after {attempts} attempts: {url}")]
RateLimited {
  url: String,
  attempts: usize,
},

#[error("I/O operation failed: {0}")]
Io(#[from] std::io::Error),

#[error("JSON operation failed: {0}")]
Json(#[from] serde_json::Error),
```

## Persistence

JSON persistence lives in `io.rs`.

Stores:

| Store               | File                      | Shape                                            |
| ------------------- | ------------------------- | ------------------------------------------------ |
| `ManufacturerStore` | `data/manufacturers.json` | `manufacturer -> manufacturer_path`              |
| `AircraftStore`     | `data/aircraft.json`      | `manufacturer -> aircraft_name -> aircraft_path` |
| `PlaneStore`        | `data/planes.json`        | `manufacturer -> aircraft_name -> PlaneItem`     |

The stores use:

* `BTreeMap` for deterministic JSON output
* `tokio::sync::Mutex` for concurrent async access
* temporary files plus rename for safer writes
* `serde_json::to_string_pretty` for readable output

## Project Structure

```text
src/
  main.rs
  crawler.rs
  error.rs
  fetcher.rs
  io.rs
  prelude.rs
  spiders/
    mod.rs
    plane_phd_manufacturers.rs
    models.rs
    plane.rs
  utils/
```

## Module Responsibilities

### `main.rs`

* parses CLI commands
* creates crawler configuration
* creates shared stores
* creates shared fetcher
* dispatches the selected spider

### `crawler.rs`

* owns crawl scheduling
* manages URL queues
* deduplicates visited URLs
* runs scraper workers
* runs processor workers
* coordinates shutdown

### `fetcher.rs`

* owns HTTP client behavior
* handles status codes
* retries rate-limited and transient failures
* returns response bodies as text

### `io.rs`

* reads and writes JSON files
* stores manufacturers, aircraft links, and full plane records

### `spiders/mod.rs`

Defines the shared `Spider` trait.

### `spiders/plane_phd_manufacturers.rs`

Scrapes manufacturer names and manufacturer page paths.

### `spiders/models.rs`

Reads manufacturer pages and scrapes aircraft/model detail links.

### `spiders/plane.rs`

Reads aircraft detail pages and extracts structured aircraft data.

## Troubleshooting

### `unexpected argument '--delay-ms' found`

The `run` command has not been migrated to the typed Clap configuration.

Confirm `main.rs` uses:

```rust
use clap::{Parser, Subcommand};
```

and defines a `RunConfig` struct containing:

```rust
#[arg(long, default_value_t = 1000)]
delay_ms: u64
```

### `429 Too Many Requests`

Reduce crawl pressure:

```bash
--delay-ms 3000
--crawl-concurrency 1
--process-concurrency 2
```

Also reduce fetcher retries during development:

```rust
max_retries: 3,
base_retry_delay: Duration::from_secs(2),
max_retry_delay: Duration::from_secs(30),
```

### Program appears to hang

Likely causes:

* long `429` retry sleeps
* high crawl concurrency
* high retry count
* full JSON file rewrite per aircraft
* a crawler worker panic before barrier shutdown
* no progress logs at `info` level

Recommended debug run:

```bash
RUST_LOG=info,crawler=debug cargo run -- run \
  --spider p \
  --delay-ms 3000 \
  --crawl-concurrency 1 \
  --process-concurrency 1 \
  --aircraft-file data/aircraft.json \
  --planes-file data/planes.json
```

### `name` has incompatible type for trait

Ensure all spider implementations match the trait signature.

If the trait is:

```rust
fn name(&self) -> String;
```

then implementations must return `String`:

```rust
fn name(&self) -> String {
  String::from("PlanesSpider")
}
```

If changing the trait to:

```rust
fn name(&self) -> &'static str;
```

then all implementations must update together.

### `snapshot is never used`

This means a store snapshot method is not currently used by the active code path.

Options:

* wire the spider to consume the store snapshot
* remove the unused method
* temporarily suppress with `#[allow(dead_code)]` while refactoring

## Development Workflow

Recommended loop:

```bash
cargo fmt
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Run a single stage:

```bash
cargo run -- run --spider m
cargo run -- run --spider mod
cargo run -- run --spider p
```

Run the full staged pipeline:

```bash
cargo run -- run \
  --spider m \
  --delay-ms 2000 \
  --crawl-concurrency 1 \
  --process-concurrency 4 \
  --manufacturers-file data/manufacturers.json

cargo run -- run \
  --spider mod \
  --delay-ms 3000 \
  --crawl-concurrency 1 \
  --process-concurrency 4 \
  --manufacturers-file data/manufacturers.json \
  --aircraft-file data/aircraft.json

cargo run -- run \
  --spider p \
  --delay-ms 3000 \
  --crawl-concurrency 1 \
  --process-concurrency 2 \
  --aircraft-file data/aircraft.json \
  --planes-file data/planes.json
```

## Notes

The crawler currently favors simplicity over maximum throughput. That is intentional during resurrection. Prioritize correctness, resumability, and observability before increasing concurrency.
