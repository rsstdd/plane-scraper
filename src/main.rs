use clap::{Arg, Command};
use std::{fs::read_dir, sync::Arc, time::Duration};

use crate::crawler::Crawler;
use crate::fetcher::{Fetcher, FetcherConfig};
use crate::io::ManufacturerStore;
use crate::prelude::*;

mod crawler;
mod error;
mod fetcher;
mod io;
mod prelude;
mod spiders;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,crawler=debug"))
    .init();

  let cli = Command::new(clap::crate_name!())
    .version(clap::crate_version!())
    .about(clap::crate_description!())
    .subcommand(Command::new("spiders").about("List all spiders"))
    .subcommand(
      Command::new("run").about("Run a spider").arg(
        Arg::new("spider")
          .short('s')
          .long("spider")
          .help("The spider to run")
          .value_name("SPIDER")
          .value_parser(["m", "mod", "p", "d"])
          .required(true),
      ),
    )
    .arg_required_else_help(true)
    .get_matches();

  if cli.subcommand_matches("spiders").is_some() {
    let spider_names = [
      "m - ManufacturersSpider",
      "mod - ModelsSpider",
      "p - PlanesSpider",
      "d",
    ];

    for name in spider_names {
      println!("{name}");
    }
  } else if let Some(matches) = cli.subcommand_matches("run") {
    let spider_name = matches
      .get_one::<String>("spider")
      .expect("required by clap");

    let crawler = Crawler::new(Duration::from_millis(200), 2, 500);
    let fetcher = Arc::new(Fetcher::new(FetcherConfig::from_env())?);
    let manufacturer_store = ManufacturerStore::load("data/manufacturers.json").await?;

    match spider_name.as_str() {
      "m" => {
        let spider = Arc::new(spiders::plane_phd_manufacturers::ManufacturersSpider::new(
          fetcher.clone(),
          manufacturer_store.clone(),
        ));

        crawler.run(spider).await;
      }
      "mod" => {
        let spider = Arc::new(spiders::models::ModelsSpider::new());
        crawler.run(spider).await;
      }
      "p" => {
        let spider = Arc::new(spiders::plane::PlanesSpider::new());
        crawler.run(spider).await;
      }
      "d" => {
        for entry in read_dir("./")?.filter_map(|entry| entry.ok()) {
          let entry: String = W(&entry).try_into()?;
          println!("{entry}");
        }
      }
      _ => return Err(Error::InvalidSpider(spider_name.to_string()).into()),
    };
  }

  Ok(())
}
