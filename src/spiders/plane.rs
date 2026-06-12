use crate::{
  error::Error,
  fetcher::Fetcher,
  io::{AircraftByManufacturerMap, PlaneByManufacturerMap, PlaneStore},
};
use async_trait::async_trait;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

const BASE_URL: &str = "https://planephd.com";

pub struct PlanesSpider {
  fetcher: Arc<Fetcher>,
  plane_store: PlaneStore,
  aircraft_by_url: BTreeMap<String, AircraftSource>,
  already_scraped: BTreeSet<(String, String)>,
}

#[derive(Debug, Clone)]
struct AircraftSource {
  manufacturer_name: String,
  aircraft_name: String,
  source_link: String,
}

impl PlanesSpider {
  pub fn new(
    fetcher: Arc<Fetcher>,
    plane_store: PlaneStore,
    aircraft_by_manufacturer: AircraftByManufacturerMap,
    existing_planes: PlaneByManufacturerMap,
  ) -> Self {
    let already_scraped = existing_planes
      .into_iter()
      .flat_map(|(manufacturer_name, planes)| {
        planes
          .into_keys()
          .map(move |aircraft_name| (manufacturer_name.clone(), aircraft_name))
      })
      .collect();

    let aircraft_by_url = aircraft_by_manufacturer
      .into_iter()
      .flat_map(|(manufacturer_name, aircraft)| {
        aircraft
          .into_iter()
          .map(move |(aircraft_name, source_link)| {
            let page_url = absolute_url(&source_link);

            (
              page_url,
              AircraftSource {
                manufacturer_name: manufacturer_name.clone(),
                aircraft_name,
                source_link,
              },
            )
          })
      })
      .collect();

    Self {
      fetcher,
      plane_store,
      aircraft_by_url,
      already_scraped,
    }
  }

  fn source_for_url(&self, url: &str) -> Result<AircraftSource, Error> {
    self
      .aircraft_by_url
      .get(url)
      .cloned()
      .ok_or_else(|| Error::Internal(format!("unknown aircraft detail URL: {url}")))
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaneImage {
  pub href: String,
  pub title: Option<String>,
  pub holder: Option<String>,
  pub dimensions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaneItem {
  pub manufacturer_name: String,
  pub aircraft_name: String,
  pub source_link: String,
  pub page_url: String,
  pub title: Option<String>,
  pub description: Option<String>,
  pub papi_price_estimate: Option<String>,
  pub for_sale_count: Option<String>,
  pub performance: BTreeMap<String, String>,
  pub weights: BTreeMap<String, String>,
  pub ownership_costs: BTreeMap<String, String>,
  pub engine: BTreeMap<String, String>,
  pub images: Vec<PlaneImage>,
}

#[async_trait]
impl super::Spider for PlanesSpider {
  type Item = PlaneItem;

  fn name(&self) -> String {
    String::from("PlanesSpider")
  }

  fn start_urls(&self) -> Vec<String> {
    let urls = self
      .aircraft_by_url
      .iter()
      .filter(|(_, source)| {
        !self.already_scraped.contains(&(
          source.manufacturer_name.clone(),
          source.aircraft_name.clone(),
        ))
      })
      .map(|(url, _)| url.clone())
      .collect::<Vec<_>>();

    log::info!(
      "queued {} aircraft detail pages; skipped {} already-scraped aircraft",
      urls.len(),
      self.already_scraped.len()
    );

    urls
  }

  async fn scrape(&self, url: String) -> Result<(Vec<PlaneItem>, Vec<String>), Error> {
    log::info!("visiting aircraft detail page: {url}");

    let source = self.source_for_url(&url)?;
    let text = self.fetcher.get_text(&url).await?;
    let document = Html::parse_document(text.as_str());

    let sections = parse_detail_sections(&document);

    if sections.is_empty() {
      return Err(Error::Internal(format!(
        "no aircraft detail sections found; page may be blocked or DOM changed: {url}"
      )));
    }

    if sections.len() < 4 {
      log::warn!(
        "expected at least 4 aircraft detail sections, found {}: {}",
        sections.len(),
        url
      );
    }

    let images = parse_images(&document);

    log::info!(
      "parsed aircraft detail page: manufacturer={}, aircraft={}, sections={}, images={}",
      source.manufacturer_name,
      source.aircraft_name,
      sections.len(),
      images.len()
    );

    let item = PlaneItem {
      manufacturer_name: source.manufacturer_name,
      aircraft_name: source.aircraft_name,
      source_link: source.source_link,
      page_url: url,
      title: first_text(&document, ".model-header h3:first-child"),
      description: first_text(&document, "#details_top .panel-body .row .col-md-6 p"),
      papi_price_estimate: parse_papi_price_estimate(&document),
      for_sale_count: first_text(&document, ".ads-count"),
      performance: sections.first().cloned().unwrap_or_default(),
      weights: sections.get(1).cloned().unwrap_or_default(),
      ownership_costs: sections.get(2).cloned().unwrap_or_default(),
      engine: sections.get(3).cloned().unwrap_or_default(),
      images,
    };

    Ok((vec![item], Vec::new()))
  }

  async fn process(&self, item: Self::Item) -> Result<(), Error> {
    log::info!(
      "saving plane details: {} / {}",
      item.manufacturer_name,
      item.aircraft_name
    );

    self.plane_store.insert(item).await
  }
}

fn parse_detail_sections(document: &Html) -> Vec<BTreeMap<String, String>> {
  let dl_selector = selector("#perforance_top dl.dl-details");

  document
    .select(&dl_selector)
    .map(parse_definition_list)
    .collect()
}

fn parse_definition_list(dl: ElementRef<'_>) -> BTreeMap<String, String> {
  let row_selector = selector("dt, dd");

  let values = dl
    .select(&row_selector)
    .map(|element| clean_text(&element.text().collect::<Vec<_>>().join(" ")))
    .filter(|text| !text.is_empty())
    .collect::<Vec<_>>();

  let mut rows = BTreeMap::new();

  for pair in values.chunks(2) {
    if let [label, value] = pair {
      rows.insert(normalize_label(label), value.to_string());
    }
  }

  rows
}

fn parse_images(document: &Html) -> Vec<PlaneImage> {
  let image_selector = selector(".my-gallery figure a");

  document
    .select(&image_selector)
    .filter_map(|element| {
      let href = element.attr("href")?;

      Some(PlaneImage {
        href: href.to_string(),
        title: element.attr("data-title").map(str::to_string),
        holder: element.attr("data-holder").map(str::to_string),
        dimensions: element.attr("data-dimensions").map(str::to_string),
      })
    })
    .collect()
}

fn parse_papi_price_estimate(document: &Html) -> Option<String> {
  let selector = selector("#papi_top .papi-content");
  let text = document
    .select(&selector)
    .next()
    .map(|element| clean_text(&element.text().collect::<Vec<_>>().join(" ")))?;

  extract_first_money_value(&text)
}

fn extract_first_money_value(text: &str) -> Option<String> {
  text
    .split_whitespace()
    .find(|part| part.starts_with('$'))
    .map(|value| {
      value
        .trim_matches(|ch: char| ch == ',' || ch == ';')
        .to_string()
    })
}

fn first_text(document: &Html, css_selector: &str) -> Option<String> {
  let selector = selector(css_selector);

  document
    .select(&selector)
    .next()
    .map(|element| clean_text(&element.text().collect::<Vec<_>>().join(" ")))
    .filter(|text| !text.is_empty())
}

fn normalize_label(label: &str) -> String {
  let mut output = String::new();
  let mut previous_was_underscore = false;

  for ch in clean_text(label)
    .trim_end_matches(':')
    .trim()
    .to_lowercase()
    .chars()
  {
    if ch.is_ascii_alphanumeric() {
      output.push(ch);
      previous_was_underscore = false;
    } else if !previous_was_underscore {
      output.push('_');
      previous_was_underscore = true;
    }
  }

  output.trim_matches('_').to_string()
}

fn clean_text(value: &str) -> String {
  value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn absolute_url(path_or_url: &str) -> String {
  if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
    path_or_url.to_string()
  } else {
    format!("{BASE_URL}{path_or_url}")
  }
}

fn selector(value: &str) -> Selector {
  Selector::parse(value).expect("valid CSS selector")
}
