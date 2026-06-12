// https://planephd.com/wizard/?modeltype=PistonSingle&min_year=1900&required_seats_min=4&ownership_cost_p_year_max=50000&purchase_price_max=1000000&min_speed=120&annual_hrs=100
// https://planephd.com/wizard/manufacturers/
use crate::{error::Error, fetcher::Fetcher, io::ManufacturerStore};
use async_trait::async_trait;
use select::{
  document::Document,
  predicate::{Class, Predicate},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct ManufacturersSpider {
  fetcher: Arc<Fetcher>,
  manufacturer_store: ManufacturerStore,
}

impl ManufacturersSpider {
  pub fn new(fetcher: Arc<Fetcher>, manufacturer_store: ManufacturerStore) -> Self {
    Self {
      fetcher,
      manufacturer_store,
    }
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
    String::from("ManufacturersSpider")
  }

  fn start_urls(&self) -> Vec<String> {
    vec!["https://planephd.com/wizard/manufacturers/".to_string()]
  }

  async fn scrape(&self, url: String) -> Result<(Vec<ManufacturerItem>, Vec<String>), Error> {
    log::info!("visiting: {url}");

    let text = self.fetcher.get_text(&url).await?;
    let document = Document::from(text.as_str());

    let mut items = Vec::new();

    for node in document.find(Class("pp-card").descendant(Class("list-group-item"))) {
      let Some(link) = node.attr("href") else {
        log::debug!("skipping manufacturer node without href");
        continue;
      };

      let name = node.text().trim().to_string();

      if name.is_empty() {
        log::debug!("skipping manufacturer node with empty name");
        continue;
      }

      items.push(ManufacturerItem {
        name,
        link: link.to_string(),
      });
    }

    Ok((items, Vec::new()))
  }

  async fn process(&self, item: Self::Item) -> Result<(), Error> {
    log::debug!("saving manufacturer: {} -> {}", item.name, item.link);

    self.manufacturer_store.insert(item.name, item.link).await
  }
}
