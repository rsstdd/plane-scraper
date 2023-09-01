use clap::{Command, Arg};
use std::{env, sync::Arc, time::Duration};

mod crawler;
mod error;
mod spiders;

use crate::crawler::Crawler;
use error::Error;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
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
                    .required(true),
            ),
        )
        .arg_required_else_help(true)
        .get_matches();

    env::set_var("RUST_LOG", "info,crawler=debug");
    env_logger::init();

    // we can safely unwrap as the argument is required
    let crawler = Crawler::new(Duration::from_millis(200), 2, 500);

    Ok(())
}
