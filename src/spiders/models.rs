use crate::{
  error::Error,
  fetcher::Fetcher,
  io::{AircraftStore, ManufacturerMap},
};
use async_trait::async_trait;
use select::{
  document::Document,
  predicate::{Class, Name, Predicate},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

const BASE_URL: &str = "https://planephd.com";

pub struct ModelsSpider {
  fetcher: Arc<Fetcher>,
  aircraft_store: AircraftStore,
  manufacturer_by_url: BTreeMap<String, String>,
}

impl ModelsSpider {
  pub fn new(
    fetcher: Arc<Fetcher>,
    aircraft_store: AircraftStore,
    manufacturers: ManufacturerMap,
  ) -> Self {
    let manufacturer_by_url = manufacturers
      .into_iter()
      .map(|(manufacturer_name, manufacturer_path)| {
        let url = absolute_url(&manufacturer_path);
        (url, manufacturer_name)
      })
      .collect();

    Self {
      fetcher,
      aircraft_store,
      manufacturer_by_url,
    }
  }

  fn manufacturer_name_for_url(&self, url: &str) -> Result<String, Error> {
    self
      .manufacturer_by_url
      .get(url)
      .cloned()
      .ok_or_else(|| Error::Internal(format!("unknown manufacturer URL: {url}")))
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelItem {
  manufacturer_name: String,
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
    self.manufacturer_by_url.keys().cloned().collect()
  }

  async fn scrape(&self, url: String) -> Result<(Vec<ModelItem>, Vec<String>), Error> {
    log::info!("visiting: {url}");

    let manufacturer_name = self.manufacturer_name_for_url(&url)?;
    let text = self.fetcher.get_text(&url).await?;
    let document = Document::from(text.as_str());

    let mut items = Vec::new();

    for node in document.find(Class("modal_content").descendant(Name("a"))) {
      let name = node.text().trim().to_string();

      if name.is_empty() {
        log::debug!("skipping model node with empty name");
        continue;
      }

      let Some(link) = node.attr("href") else {
        log::debug!("skipping model node without href");
        continue;
      };

      items.push(ModelItem {
        manufacturer_name: manufacturer_name.clone(),
        name,
        link: link.trim().to_string(),
      });
    }

    Ok((items, Vec::new()))
  }

  async fn process(&self, item: Self::Item) -> Result<(), Error> {
    log::debug!(
      "saving aircraft: {} / {} -> {}",
      item.manufacturer_name,
      item.name,
      item.link
    );

    self
      .aircraft_store
      .insert(item.manufacturer_name, item.name, item.link)
      .await
  }
}

fn absolute_url(path_or_url: &str) -> String {
  if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
    path_or_url.to_string()
  } else {
    format!("{BASE_URL}{path_or_url}")
  }
}
