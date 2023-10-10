// https://planephd.com/wizard/?modeltype=PistonSingle&min_year=1900&required_seats_min=4&ownership_cost_p_year_max=50000&purchase_price_max=1000000&min_speed=120&annual_hrs=100
// https://planephd.com/wizard/manufacturers/
use crate::error::Error;
use async_trait::async_trait;
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use std::time::Duration;
// use scraper::{Html, Selector};
use select::{
  document::Document,
  predicate::{Class, Predicate},
};

pub struct ManufacturersSpider {
  http_client: Client,
}

impl ManufacturersSpider {
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

    ManufacturersSpider { http_client }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManufacturerItem {
  name: String,
  link: String,
}

#[async_trait]
impl super::Spider for ManufacturersSpider {
  type Item = ManufacturerItem;

  fn name(&self) -> String {
    String::from("manufacturer")
  }

  fn start_urls(&self) -> Vec<String> {
    vec!["https://planephd.com/wizard/manufacturers/".to_string()]
  }

  async fn scrape(&self, url: String) -> Result<(Vec<ManufacturerItem>, Vec<String>), Error> {
    log::info!("visiting: {}", url);
    let res = self.http_client.get(&url).send().await?;
    let text = res.text().await?;

    let mut items = Vec::new();
    let document = Document::from(text.as_str());

    for node in document.find(Class("pp-card").descendant(Class("list-group-item"))) {
      println!("{:?}", &node.attr("href").unwrap().to_string());
      let plane = ManufacturerItem {
        name: node.text(),
        link: node.attr("href").unwrap().to_string(),
      };
      items.push(plane);
    }

    println!("{:?}", items);

    Ok((items, Vec::new()))
  }

  async fn process(&self, item: Self::Item) -> Result<(), Error> {
    println!("{}", item.name);

    Ok(())
  }
}
