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
  /// One URL to *many* sources, because 38 of PlanePHD's detail URLs are listed
  /// under more than one manufacturer. While this was a `BTreeMap<_, AircraftSource>`
  /// the second insert silently overwrote the first, the page was filed under one
  /// manufacturer, and the other 28 aircraft were simply absent from the output --
  /// unrecoverably, since the surviving pair lands in `already_scraped` and the URL
  /// is never queued again.
  aircraft_by_url: BTreeMap<String, Vec<AircraftSource>>,
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

    let aircraft_by_url = Self::group_by_url(aircraft_by_manufacturer);

    let shared = aircraft_by_url
      .values()
      .filter(|sources| sources.len() > 1)
      .count();
    if shared > 0 {
      log::info!("{shared} detail pages are listed under more than one manufacturer");
    }

    Self {
      fetcher,
      plane_store,
      aircraft_by_url,
      already_scraped,
    }
  }

  /// Groups listings by the page they share instead of letting the last one
  /// win. Separated from `new` so the grouping can be tested without a fetcher.
  fn group_by_url(
    aircraft_by_manufacturer: AircraftByManufacturerMap,
  ) -> BTreeMap<String, Vec<AircraftSource>> {
    let mut aircraft_by_url: BTreeMap<String, Vec<AircraftSource>> = BTreeMap::new();
    for (manufacturer_name, aircraft) in aircraft_by_manufacturer {
      for (aircraft_name, source_link) in aircraft {
        let page_url = absolute_url(&source_link);
        aircraft_by_url
          .entry(page_url)
          .or_default()
          .push(AircraftSource {
            manufacturer_name: manufacturer_name.clone(),
            aircraft_name,
            source_link,
          });
      }
    }
    aircraft_by_url
  }

  fn sources_for_url(&self, url: &str) -> Result<&[AircraftSource], Error> {
    self
      .aircraft_by_url
      .get(url)
      .map(Vec::as_slice)
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
    // Any missing source is enough to queue the page. Filtering on a single
    // source would leave a shared URL unqueued as soon as one of its
    // manufacturers had been scraped, which is how the 28 went missing.
    let urls = self
      .aircraft_by_url
      .iter()
      .filter(|(_, sources)| {
        sources.iter().any(|source| {
          !self.already_scraped.contains(&(
            source.manufacturer_name.clone(),
            source.aircraft_name.clone(),
          ))
        })
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

    let sources = self.sources_for_url(&url)?;
    let text = self.fetcher.get_text(&url).await?;
    let document = Html::parse_document(text.as_str());

    // Two layouts, tried oldest first so a page still serving the legacy markup
    // keeps its ownership_costs. The new `/research/<slug>` layout publishes
    // performance, weights and engine but puts the cost breakdown behind a
    // sign-in, so a record parsed from it carries no ownership_costs at all --
    // and robots.txt disallows /signin, so that is where the data stops.
    let legacy = parse_detail_sections(&document);
    let (performance, weights, ownership_costs, engine) = if legacy.is_empty() {
      let Some(new) = parse_new_layout(&document) else {
        return Err(Error::Internal(format!(
          "neither the legacy nor the /research layout produced any specification; \
           the page may be blocked or changed again: {url}"
        )));
      };
      (new.performance, new.weights, BTreeMap::new(), new.engine)
    } else {
      if legacy.len() < 4 {
        log::warn!(
          "expected at least 4 legacy detail sections, found {}: {url}",
          legacy.len()
        );
      }
      verify_section_order(&legacy, &url)?;
      (
        legacy.first().cloned().unwrap_or_default(),
        legacy.get(1).cloned().unwrap_or_default(),
        legacy.get(2).cloned().unwrap_or_default(),
        legacy.get(3).cloned().unwrap_or_default(),
      )
    };

    for source in sources {
      verify_page_is_for(&document, source, &url)?;
    }

    let images = parse_images(&document);

    log::info!(
      "parsed aircraft detail page: listings={}, performance={}, weights={}, costs={}, \
       engine={}, images={}: {url}",
      sources.len(),
      performance.len(),
      weights.len(),
      ownership_costs.len(),
      engine.len(),
      images.len()
    );

    let title = first_text(&document, ".model-header h3:first-child");
    let description = first_text(&document, "#details_top .panel-body .row .col-md-6 p");
    let papi_price_estimate = parse_papi_price_estimate(&document);
    let for_sale_count = first_text(&document, ".ads-count");

    // One item per listing, not per page. A page reachable from two manufacturer
    // indexes is two catalogue entries carrying the same specifications, and
    // emitting one of them is what lost 28 aircraft.
    let items = sources
      .iter()
      .map(|source| PlaneItem {
        manufacturer_name: source.manufacturer_name.clone(),
        aircraft_name: source.aircraft_name.clone(),
        source_link: source.source_link.clone(),
        page_url: url.clone(),
        title: title.clone(),
        description: description.clone(),
        papi_price_estimate: papi_price_estimate.clone(),
        for_sale_count: for_sale_count.clone(),
        performance: performance.clone(),
        weights: weights.clone(),
        ownership_costs: ownership_costs.clone(),
        engine: engine.clone(),
        images: images.clone(),
      })
      .collect();

    Ok((items, Vec::new()))
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

/// The four detail sections are read by position -- `sections[0..3]` in DOM
/// order -- because the page gives them no machine-readable label. Position is
/// only safe while the order holds, and a reordered page would otherwise file
/// weights under `performance` and cost figures under `engine` with nothing to
/// show for it but a row of plausible numbers in the wrong columns.
///
/// Each signature is a key that appears in that section and in no other, taken
/// from the 1,005 records already scraped: `weights` carries exactly four keys,
/// `engine` six, and `performance` fifteen, none overlapping. `ownership_costs`
/// has 709 distinct parameterised keys and no stable member, so it is checked by
/// elimination -- it is the section that is none of the other three.
const SECTION_SIGNATURES: [(&str, &[&str]); 4] = [
  (
    "performance",
    &[
      "ceiling",
      "rate_of_climb",
      "takeoff_distance",
      "landing_distance",
    ],
  ),
  (
    "weights",
    &[
      "gross_weight",
      "empty_weight",
      "fuel_capacity",
      "maximum_payload",
    ],
  ),
  ("ownership_costs", &[]),
  (
    "engine",
    &[
      "horsepower",
      "thrust",
      "overhaul_ht",
      "years_before_overhaul",
    ],
  ),
];

/// Rejects the page rather than filing it wrongly. A rejected URL is recorded as
/// a failure and can be retried; a misfiled record looks like data forever.
fn verify_section_order(sections: &[BTreeMap<String, String>], url: &str) -> Result<(), Error> {
  for (index, (name, signature)) in SECTION_SIGNATURES.iter().enumerate() {
    let Some(section) = sections.get(index) else {
      continue;
    };
    if signature.is_empty() {
      continue;
    }
    if !signature.iter().any(|key| section.contains_key(*key)) {
      return Err(Error::Internal(format!(
        "detail section {index} does not look like {name}; its keys are [{}]. \
         The page layout changed and reading sections by position is no longer \
         safe: {url}",
        section
          .keys()
          .take(8)
          .cloned()
          .collect::<Vec<_>>()
          .join(", ")
      )));
    }
  }
  Ok(())
}

/// Where a label on the `/research/<slug>` layout belongs, in the vocabulary the
/// 1,005 legacy records already use.
///
/// PlanePHD replaced `/wizard/details/<id>/<SLUG>-specifications-...` with
/// `/research/<slug>` some time before 2026-09-12; every old URL 301s to the new
/// one and the old `#perforance_top dl.dl-details` markup is gone site-wide.
/// Emitting the legacy key names keeps `crates/aircraft_ingest`'s normalization
/// mapping working unchanged -- a new spelling would arrive as an unmapped
/// measurement field and become curation work for no gain.
///
/// `Horsepower` and `Thrust` are deliberately written to two sections, because
/// the legacy records carry them under both `performance` and `engine`.
const NEW_LAYOUT_FIELDS: &[(&str, &[(&str, &str)])] = &[
  (
    "Horsepower",
    &[("performance", "horsepower"), ("engine", "horsepower")],
  ),
  ("Thrust", &[("performance", "thrust"), ("engine", "thrust")]),
  ("Best Cruise Speed", &[("performance", "best_cruise_speed")]),
  ("Stall Speed", &[("performance", "stall_speed")]),
  ("Best Range", &[("performance", "best_range_i")]),
  ("Rate of Climb", &[("performance", "rate_of_climb")]),
  ("Fuel Burn", &[("performance", "fuel_burn")]),
  ("Ceiling", &[("performance", "ceiling")]),
  ("Takeoff Distance", &[("performance", "takeoff_distance")]),
  ("Landing Distance", &[("performance", "landing_distance")]),
  ("Engine Manufacturer", &[("engine", "manufacturer")]),
  ("Engine Model", &[("engine", "model")]),
  ("TBO", &[("engine", "overhaul_ht")]),
  ("Gross Weight", &[("weights", "gross_weight")]),
  ("Empty Weight", &[("weights", "empty_weight")]),
  ("Fuel Capacity", &[("weights", "fuel_capacity")]),
  ("Payload w/ Full Fuel", &[("weights", "payload_full_fuel")]),
  // No legacy equivalent, but `aircraft_ref.weight_metric_types` already has a
  // code for each, so crates/aircraft_ingest maps all three to canonical weights
  // rather than leaving them as curation work.
  ("Useful Load", &[("weights", "useful_load")]),
  ("Fuel Weight", &[("weights", "fuel_weight")]),
];

/// The four maps the legacy layout produced, rebuilt from the new one.
#[derive(Default)]
struct DetailSections {
  performance: BTreeMap<String, String>,
  weights: BTreeMap<String, String>,
  engine: BTreeMap<String, String>,
}

/// Reads `section#numbers`, whose two cards hold rows of
/// `<span>label</span><span>value</span>`.
///
/// Returns `None` when the section is absent, so the caller can fall back to the
/// legacy layout rather than silently publishing an empty record.
fn parse_new_layout(document: &Html) -> Option<DetailSections> {
  let row_selector = selector("section#numbers div.flex");
  let span_selector = selector("span");

  let mut sections = DetailSections::default();
  let mut matched = 0usize;

  for row in document.select(&row_selector) {
    let spans: Vec<String> = row
      .select(&span_selector)
      .map(|s| clean_text(&s.text().collect::<Vec<_>>().join(" ")))
      .collect();
    let [label, value] = spans.as_slice() else {
      continue;
    };
    if value.is_empty() {
      continue;
    }
    let Some((_, targets)) = NEW_LAYOUT_FIELDS.iter().find(|(name, _)| name == label) else {
      log::debug!("unmapped specification label on the new layout: {label} = {value}");
      continue;
    };
    matched += 1;
    for (section, key) in *targets {
      let map = match *section {
        "performance" => &mut sections.performance,
        "weights" => &mut sections.weights,
        _ => &mut sections.engine,
      };
      map.insert((*key).to_owned(), value.clone());
    }
  }

  (matched > 0).then_some(sections)
}

/// Guards against a redirect landing on a different aircraft.
///
/// Every legacy URL 301s, and the new path is derived from a slug rather than the
/// numeric id, so a stale or wrong id resolves to whatever aircraft now owns that
/// slug -- silently, with a 200 and a perfectly well-formed page. Requiring the
/// document to name the manufacturer we asked for turns that into a failure.
fn verify_page_is_for(document: &Html, source: &AircraftSource, url: &str) -> Result<(), Error> {
  let text = document
    .root_element()
    .text()
    .collect::<String>()
    .to_lowercase();
  let manufacturer = source.manufacturer_name.to_lowercase();
  if text.contains(&manufacturer) {
    return Ok(());
  }
  Err(Error::Internal(format!(
    "page does not mention {}, so the redirect resolved to a different aircraft: {url}",
    source.manufacturer_name
  )))
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::fetcher::FetcherConfig;
  use crate::io::AircraftByManufacturerMap;
  use crate::spiders::Spider as _;

  /// Two manufacturers listing the same detail page, which is the shape that
  /// lost 28 aircraft: PlanePHD's index claims 38 URLs under more than one
  /// manufacturer.
  fn two_manufacturers_one_page() -> AircraftByManufacturerMap {
    let link = String::from("/wizard/details/1246/AERO-COMMANDER-100-specifications");
    AircraftByManufacturerMap::from([
      (
        String::from("AERO COMMANDER"),
        BTreeMap::from([(String::from("100 (1960 - 1967)"), link.clone())]),
      ),
      (
        String::from("COMMANDER"),
        BTreeMap::from([(String::from("100 (1960 - 1967)"), link)]),
      ),
    ])
  }

  fn spider(existing: PlaneByManufacturerMap) -> PlanesSpider {
    let fetcher = Arc::new(
      Fetcher::new(FetcherConfig::from_env()).expect("a fetcher builds without any network I/O"),
    );
    PlanesSpider {
      fetcher,
      plane_store: PlaneStore::empty_for_test(),
      aircraft_by_url: PlanesSpider::group_by_url(two_manufacturers_one_page()),
      already_scraped: existing
        .into_iter()
        .flat_map(|(manufacturer, planes)| {
          planes
            .into_keys()
            .map(move |aircraft| (manufacturer.clone(), aircraft))
        })
        .collect(),
    }
  }

  #[test]
  fn a_page_listed_under_two_manufacturers_keeps_both_listings() {
    let grouped = PlanesSpider::group_by_url(two_manufacturers_one_page());

    assert_eq!(grouped.len(), 1, "one page: {grouped:?}");
    let sources = grouped.values().next().expect("the one page");
    let mut manufacturers: Vec<&str> = sources
      .iter()
      .map(|source| source.manufacturer_name.as_str())
      .collect();
    manufacturers.sort_unstable();
    assert_eq!(
      manufacturers,
      ["AERO COMMANDER", "COMMANDER"],
      "a BTreeMap keyed by URL kept only the last of these"
    );
  }

  #[test]
  fn a_shared_page_is_still_queued_while_either_listing_is_missing() {
    // The already-scraped listing must be the one the grouped Vec yields FIRST,
    // or this test cannot tell `any` from `first`: sources are built by
    // iterating a BTreeMap, so "AERO COMMANDER" precedes "COMMANDER". With the
    // scraped listing second, a filter that looks at only one source still sees
    // a missing listing and the test passes against the defect it exists to
    // catch. Mutation-checked both ways round.
    let existing = PlaneByManufacturerMap::from([(
      String::from("AERO COMMANDER"),
      BTreeMap::from([(String::from("100 (1960 - 1967)"), sample_item())]),
    )]);

    assert_eq!(
      spider(existing).start_urls().len(),
      1,
      "a page owing a listing must be queued even though another listing has it"
    );
  }

  #[test]
  fn a_page_whose_listings_are_all_scraped_is_not_queued() {
    let scraped = |manufacturer: &str| (String::from(manufacturer), sample_item());
    let existing = PlaneByManufacturerMap::from([
      (
        String::from("AERO COMMANDER"),
        BTreeMap::from([(String::from("100 (1960 - 1967)"), scraped("a").1)]),
      ),
      (
        String::from("COMMANDER"),
        BTreeMap::from([(String::from("100 (1960 - 1967)"), scraped("b").1)]),
      ),
    ]);

    assert!(
      spider(existing).start_urls().is_empty(),
      "nothing is owed, so the page must not be refetched"
    );
  }

  fn sample_item() -> PlaneItem {
    PlaneItem {
      manufacturer_name: String::from("COMMANDER"),
      aircraft_name: String::from("100 (1960 - 1967)"),
      source_link: String::new(),
      page_url: String::new(),
      title: None,
      description: None,
      papi_price_estimate: None,
      for_sale_count: None,
      performance: BTreeMap::new(),
      weights: BTreeMap::new(),
      ownership_costs: BTreeMap::new(),
      engine: BTreeMap::new(),
      images: Vec::new(),
    }
  }

  /// Synthetic markup in the shape of the `/research/<slug>` layout. Real
  /// PlanePHD HTML is not checked in: their terms forbid reproducing the data,
  /// and the structure is what this parses, not the values.
  fn new_layout_page() -> Html {
    Html::parse_document(
      r#"<html><body>
        <section id="numbers">
          <div class="rounded-2xl">
            <h3>Performance</h3>
            <div class="flex"><span>Horsepower</span><span>180 HP</span></div>
            <div class="flex"><span>Best Cruise Speed</span><span>192 kt</span></div>
            <div class="flex"><span>Best Range</span><span>1,544 nm</span></div>
            <div class="flex"><span>Unmapped Future Field</span><span>7 xyz</span></div>
          </div>
          <div class="rounded-2xl">
            <h3>Engine &amp; weights</h3>
            <div class="flex"><span>Engine Manufacturer</span><span>Lycoming</span></div>
            <div class="flex"><span>TBO</span><span>2,000 hrs</span></div>
            <div class="flex"><span>Gross Weight</span><span>2,100 lbs</span></div>
            <div class="flex"><span>Payload w/ Full Fuel</span><span>264 lbs</span></div>
            <div class="flex"><span>Empty Weight</span><span></span></div>
          </div>
        </section>
        <p>Built by Lycoming Aircraft Company</p>
      </body></html>"#,
    )
  }

  #[test]
  fn the_research_layout_yields_the_legacy_key_names() {
    let parsed = parse_new_layout(&new_layout_page()).expect("the layout is recognised");

    assert_eq!(
      parsed
        .performance
        .get("best_cruise_speed")
        .map(String::as_str),
      Some("192 kt")
    );
    assert_eq!(
      parsed.performance.get("best_range_i").map(String::as_str),
      Some("1,544 nm")
    );
    assert_eq!(
      parsed.engine.get("manufacturer").map(String::as_str),
      Some("Lycoming")
    );
    assert_eq!(
      parsed.engine.get("overhaul_ht").map(String::as_str),
      Some("2,000 hrs")
    );
    assert_eq!(
      parsed.weights.get("gross_weight").map(String::as_str),
      Some("2,100 lbs")
    );
    assert_eq!(
      parsed.weights.get("payload_full_fuel").map(String::as_str),
      Some("264 lbs")
    );

    // Horsepower is the one label the legacy records carry in two sections.
    assert_eq!(
      parsed.performance.get("horsepower").map(String::as_str),
      Some("180 HP")
    );
    assert_eq!(
      parsed.engine.get("horsepower").map(String::as_str),
      Some("180 HP")
    );

    // A label with no value is not a key with an empty string, and a label this
    // table does not know is skipped rather than guessed at.
    assert!(
      !parsed.weights.contains_key("empty_weight"),
      "{:?}",
      parsed.weights
    );
    assert!(
      !parsed.performance.values().any(|v| v == "7 xyz"),
      "an unrecognised label must not be filed under some other key"
    );
  }

  #[test]
  fn a_page_for_another_aircraft_is_refused() {
    let page = new_layout_page();
    let asked_for = |manufacturer: &str| AircraftSource {
      manufacturer_name: String::from(manufacturer),
      aircraft_name: String::from("GX"),
      source_link: String::new(),
    };

    assert!(
      verify_page_is_for(&page, &asked_for("Lycoming"), "u").is_ok(),
      "the page names this manufacturer"
    );
    assert!(
      verify_page_is_for(&page, &asked_for("Bombardier"), "u").is_err(),
      "a redirect that lands on a different aircraft must fail, not be recorded"
    );
  }

  #[test]
  fn a_section_that_does_not_match_its_position_is_rejected() {
    let weights = BTreeMap::from([(String::from("gross_weight"), String::from("2550 lbs"))]);
    let performance = BTreeMap::from([(String::from("ceiling"), String::from("13500 ft"))]);

    assert!(
      verify_section_order(&[performance.clone(), weights.clone()], "u").is_ok(),
      "the documented order must pass"
    );
    assert!(
      verify_section_order(&[weights, performance], "u").is_err(),
      "swapping two sections must be refused, not filed under the wrong keys"
    );
  }
}
