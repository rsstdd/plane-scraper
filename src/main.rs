#![allow(unused)]

use clap::{Arg, Command};
use std::{env, sync::Arc, time::Duration};

use crate::crawler::Crawler;
use crate::prelude::*;
use std::fs::read_dir;

mod crawler;
mod error;
mod prelude;
mod spiders;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
  env::set_var("RUST_LOG", "info,crawler=debug");
  env_logger::init();

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
          .takes_value(true)
          .required(true),
      ),
    )
    .arg_required_else_help(true)
    .get_matches();

  if let Some(_) = cli.subcommand_matches("spiders") {
    let spider_names = vec!["m"];
    for name in spider_names {
      println!("{}", name);
    }
  } else if let Some(matches) = cli.subcommand_matches("run") {
    let spider_name = matches.value_of("spider").unwrap();
    let crawler = Crawler::new(Duration::from_millis(200), 2, 500);

    match spider_name {
      "m" => {
        let spider = Arc::new(spiders::plane_phd_manufacturers::ManufacturersSpider::new());
        crawler.run(spider).await;
      },
      "mod" => {
        let spider = Arc::new(spiders::models::ModelsSpider::new());
        crawler.run(spider).await;
      },
      "p" => {
        let spider = Arc::new(spiders::plane::PlanesSpider::new());
        crawler.run(spider).await;
      },
      "d" => {
        for entry in read_dir("./").unwrap().filter_map(|e| e.ok()) {
          let entry: String = W(&entry).try_into()?;
          println!("{entry}");
        }
      },
      _ => return Err(Error::InvalidSpider(spider_name.to_string()).into()),
    };
  }

  Ok(())
}
