use crate::error::Error;
use async_trait::async_trait;
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use std::time::Duration;
// use scraper::{Html, Selector};
use select::{
  document::Document,
  predicate::{Class, Name, Predicate},
};

pub struct ModelsSpider {
  http_client: Client,
}

impl ModelsSpider {
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

    ModelsSpider { http_client }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelItem {
  name: String,
  link: String,
}

#[async_trait]
impl super::Spider for ModelsSpider {
  type Item = ModelItem;

  fn name(&self) -> String {
    String::from("models")
  }

  fn start_urls(&self) -> Vec<String> {
    vec!["https://planephd.com/wizard/manufacturers/CESSNA/".to_string()]
  }

  async fn scrape(&self, url: String) -> Result<(Vec<ModelItem>, Vec<String>), Error> {
    log::info!("visiting: {}", url);
    let res = self.http_client.get(&url).send().await?;
    let text = res.text().await?;

    let mut items = Vec::new();
    let document = Document::from(text.as_str());

    for node in document.find(Class("modal_content").descendant(Name("a"))) {
      let name = node.text().trim().to_string();
      let link = node.attr("href").unwrap().trim().to_string();
      items.push(ModelItem { name, link });
    }

    Ok((items, Vec::new()))
  }

  async fn process(&self, item: Self::Item) -> Result<(), Error> {
    println!("{}", item.name);
    println!("{}", item.link);
    println!("\n");

    Ok(())
  }
}
