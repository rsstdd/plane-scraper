use clap::{Command, Arg};
use std::{env, sync::Arc, time::Duration};

mod crawler;
mod error;
mod spiders;

use crate::crawler::Crawler;
// use error::Error;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env::set_var("RUST_LOG", "info,crawler=debug");
    env_logger::init();

    // we can safely unwrap as the argument is required
    let crawler = Crawler::new(Duration::from_millis(200), 2, 500);
    let spider = Arc::new(spiders::plane_phd::PlanePhdSpider::new());
    crawler.run(spider).await;

    Ok(())
}
