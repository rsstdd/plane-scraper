use crate::error::Error;
use async_trait::async_trait;
use reqwest::{header, Client};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub struct PlanesSpider {
  http_client: Client,
}

impl PlanesSpider {
  pub fn new() -> Self {
    let http_timeout = Duration::from_secs(6);
    let mut headers = header::HeaderMap::new();
    headers.insert("Accept", header::HeaderValue::from_static("application/json"));

    let http_client = Client::builder()
      .timeout(http_timeout)
      .default_headers(headers)
      .user_agent("Mozilla/5.0 (Windows NT 6.1; Win64; x64; rv:47.0) Gecko/20100101 Firefox/47.0")
      .build()
      .expect("spiders/github: Building HTTP client");

    PlanesSpider { http_client }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanePowerplant {
  manufacturer: String,
  model: String,
  horsepower: String,
  tbo: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaneWeight {
  gross_weight: String,
  empty_weight: String,
  fuel_capacity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaneItem {
  name: String,
  description: String,
  link: String,
  horsepower: String,
  curise_speed: String,
  range: String,
  fuel_burn_75: String,
  stall_speed: String,
  rate_of_climb: String,
  ceiling: String,
  takeoff_distance: String,
  landing_distance: String,
  takeoff_distance_over_50: String,
  landing_distance_over_50: String,
  weight: PlaneWeight,
  powerplant: PlanePowerplant,
}

#[async_trait]
impl super::Spider for PlanesSpider {
  type Item = PlaneItem;

  fn name(&self) -> String {
    String::from("PlanesSpider")
  }

  fn start_urls(&self) -> Vec<String> {
    vec![
      "https://planephd.com/wizard/details/146/CESSNA-120-specifications-performance-operating-cost-valuation"
        .to_string(),
    ]
  }

  async fn scrape(&self, url: String) -> Result<(Vec<PlaneItem>, Vec<String>), Error> {
    log::info!("visiting: {}", url);
    let res = self.http_client.get(&url).send().await?;
    let text = res.text().await?;

    let mut _items = Vec::new();
    let document = Html::parse_document(&text.as_str());

    let title_selector = Selector::parse("h3:first-of-type").unwrap();
    let title = document.select(&title_selector).next().unwrap().inner_html();

    let dl_selector = Selector::parse("div dl").unwrap();
    let dl = document.select(&dl_selector).next().unwrap();
    let dt_selector = Selector::parse(
      "#perforance_top > div > div > div > dl > dt > p, #perforance_top > div > div > div > dl > dd > p",
    )
    .unwrap();

    println!("\nTITLE: {:#?}\n", title);

    for el in dl.select(&dt_selector) {
      let text = el.inner_html();
      println!("==> {:#?} ", text);
    }

    println!("\nWEIGHTS");

    let weights_selector =
      Selector::parse("#perforance_top > div:nth-child(1) > div:nth-child(1) > div > dl:nth-child(7)").unwrap();
    let weights = document.select(&weights_selector).next().unwrap();
    for el in weights.select(&dt_selector).into_iter() {
      let text = el.inner_html();
      println!("==> {:#?}", text);
    }

    println!("\nENGINE");

    let engine_selector = Selector::parse("#perforance_top > div:nth-child(1) > div:nth-child(3) > div > dl").unwrap();
    let engine = document.select(&engine_selector).next().unwrap();
    for el in engine.select(&dt_selector).into_iter() {
      let text = el.inner_html();
      println!("==> {:#?}", text);
    }

    println!("====");

    Ok((_items, Vec::new()))
  }

  async fn process(&self, _item: Self::Item) -> Result<(), Error> {
    println!("\n");
    println!("\n");

    Ok(())
  }
}
