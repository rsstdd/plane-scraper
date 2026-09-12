use clap::{Parser, Subcommand};
use std::{fs::read_dir, path::PathBuf, sync::Arc, time::Duration};

use crate::{
  crawler::Crawler,
  fetcher::{Fetcher, FetcherConfig},
  io::{AircraftStore, ManufacturerStore, PlaneStore},
  prelude::*,
  spiders::Spider,
};

mod crawler;
mod error;
mod fetcher;
mod io;
mod prelude;
mod spiders;
mod utils;

#[derive(Debug, Parser)]
#[command(name = clap::crate_name!())]
#[command(version = clap::crate_version!())]
#[command(about = clap::crate_description!())]
struct Cli {
  #[command(subcommand)]
  command: CliCommand,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
  /// List all spiders
  Spiders,

  /// Run a spider
  Run(RunConfig),
}

#[derive(Debug, Parser)]
struct RunConfig {
  /// The spider to run
  #[arg(short, long, value_parser = ["m", "mod", "p", "d"])]
  spider: String,

  /// Delay between crawl operations in milliseconds
  #[arg(long, default_value_t = 1000)]
  delay_ms: u64,

  /// Number of concurrent page crawlers
  #[arg(long, default_value_t = 1)]
  crawl_concurrency: usize,

  /// Number of concurrent item processors
  #[arg(long, default_value_t = 32)]
  process_concurrency: usize,

  /// JSON file containing manufacturer name -> manufacturer URL path
  #[arg(long, default_value = "data/manufacturers.json")]
  manufacturers_file: PathBuf,

  /// JSON file containing manufacturer -> aircraft name -> aircraft URL path
  #[arg(long, default_value = "data/aircraft.json")]
  aircraft_file: PathBuf,

  /// JSON file containing full aircraft detail records
  #[arg(long, default_value = "data/planes.json")]
  planes_file: PathBuf,

  /// JSON file listing the URLs this run could not turn into a record
  #[arg(long, default_value = "data/failures.json")]
  failures_file: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,crawler=debug"))
    .init();

  let cli = Cli::parse();

  match cli.command {
    CliCommand::Spiders => list_spiders(),
    CliCommand::Run(config) => run_spider(config).await?,
  }

  Ok(())
}

fn list_spiders() {
  let spider_names = [
    "m - ManufacturersSpider",
    "mod - ModelsSpider",
    "p - PlanesSpider",
    "d - Debug directory listing",
  ];

  for name in spider_names {
    println!("{name}");
  }
}

async fn run_spider(config: RunConfig) -> Result<()> {
  let crawler = Crawler::new(
    Duration::from_millis(config.delay_ms),
    config.crawl_concurrency,
    config.process_concurrency,
    config.failures_file.clone(),
  );

  log::info!(
    "crawler config: delay_ms={}, crawl_concurrency={}, process_concurrency={}",
    config.delay_ms,
    config.crawl_concurrency,
    config.process_concurrency
  );

  let fetcher = Arc::new(Fetcher::new(FetcherConfig::from_env())?);

  let manufacturer_store = ManufacturerStore::load(config.manufacturers_file).await?;
  let aircraft_store = AircraftStore::load(config.aircraft_file).await?;
  let planes_store = PlaneStore::load(config.planes_file).await?;

  match config.spider.as_str() {
    "m" => {
      let spider = Arc::new(spiders::plane_phd_manufacturers::ManufacturersSpider::new(
        fetcher.clone(),
        manufacturer_store.clone(),
      ));

      log::info!("running spider: {}", spider.name());
      crawler.run(spider).await;
    }

    "mod" => {
      let manufacturers = manufacturer_store.snapshot().await;

      let spider = Arc::new(spiders::models::ModelsSpider::new(
        fetcher.clone(),
        aircraft_store.clone(),
        manufacturers,
      ));

      log::info!("running spider: {}", spider.name());
      crawler.run(spider).await;
    }

    "p" => {
      let aircraft = aircraft_store.snapshot().await;
      let existing_planes = planes_store.snapshot().await;

      let spider = Arc::new(spiders::plane::PlanesSpider::new(
        fetcher.clone(),
        planes_store.clone(),
        aircraft,
        existing_planes,
      ));

      log::info!("running spider: {}", spider.name());
      crawler.run(spider).await;
    }

    "d" => {
      for entry in read_dir("./")? {
        let entry = entry?;
        println!("{}", entry.path().display());
      }
    }

    _ => return Err(Error::InvalidSpider(config.spider)),
  }

  Ok(())
}

//
// cargo run -- run \
// --spider mod \
// --delay-ms 1000 \
// --crawl-concurrency 1 \
// --process-concurrency 32 \
// --manufacturers-file data/manufacturers.json \
// --aircraft-file data/aircraft.json
