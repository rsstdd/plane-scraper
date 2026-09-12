use crate::{prelude::*, spiders::plane::PlaneItem};
use std::{
  collections::BTreeMap,
  io::ErrorKind,
  path::{Path, PathBuf},
  sync::Arc,
  time::{SystemTime, UNIX_EPOCH},
};
use tokio::{fs, sync::Mutex};

pub type ManufacturerMap = BTreeMap<String, String>;
pub type AircraftByManufacturerMap = BTreeMap<String, BTreeMap<String, String>>;
pub type PlaneByManufacturerMap = BTreeMap<String, BTreeMap<String, PlaneItem>>;

#[derive(Clone, Debug)]
pub struct ManufacturerStore {
  path: PathBuf,
  manufacturers: Arc<Mutex<ManufacturerMap>>,
}

impl ManufacturerStore {
  pub async fn load(path: impl Into<PathBuf>) -> Result<Self> {
    let path = path.into();
    let manufacturers = read_json(&path).await?;

    Ok(Self {
      path,
      manufacturers: Arc::new(Mutex::new(manufacturers)),
    })
  }

  pub async fn insert(&self, name: impl Into<String>, link: impl Into<String>) -> Result<()> {
    let mut manufacturers = self.manufacturers.lock().await;

    manufacturers.insert(name.into(), link.into());

    write_json(&self.path, &*manufacturers).await
  }

  pub async fn insert_many<I, N, L>(&self, entries: I) -> Result<()>
  where
    I: IntoIterator<Item = (N, L)>,
    N: Into<String>,
    L: Into<String>,
  {
    let mut manufacturers = self.manufacturers.lock().await;

    for (name, link) in entries {
      manufacturers.insert(name.into(), link.into());
    }

    write_json(&self.path, &*manufacturers).await
  }

  pub async fn snapshot(&self) -> ManufacturerMap {
    self.manufacturers.lock().await.clone()
  }
}

#[derive(Clone, Debug)]
pub struct AircraftStore {
  path: PathBuf,
  aircraft_by_manufacturer: Arc<Mutex<AircraftByManufacturerMap>>,
}

impl AircraftStore {
  pub async fn load(path: impl Into<PathBuf>) -> Result<Self> {
    let path = path.into();
    let aircraft_by_manufacturer = read_json(&path).await?;

    Ok(Self {
      path,
      aircraft_by_manufacturer: Arc::new(Mutex::new(aircraft_by_manufacturer)),
    })
  }

  pub async fn insert(
    &self,
    manufacturer_name: impl Into<String>,
    aircraft_name: impl Into<String>,
    aircraft_link: impl Into<String>,
  ) -> Result<()> {
    let mut aircraft_by_manufacturer = self.aircraft_by_manufacturer.lock().await;

    aircraft_by_manufacturer
      .entry(manufacturer_name.into())
      .or_default()
      .insert(aircraft_name.into(), aircraft_link.into());

    write_json(&self.path, &*aircraft_by_manufacturer).await
  }

  pub async fn insert_many<I, N, L>(
    &self,
    manufacturer_name: impl Into<String>,
    aircraft: I,
  ) -> Result<()>
  where
    I: IntoIterator<Item = (N, L)>,
    N: Into<String>,
    L: Into<String>,
  {
    let manufacturer_name = manufacturer_name.into();
    let mut aircraft_by_manufacturer = self.aircraft_by_manufacturer.lock().await;

    let aircraft_map = aircraft_by_manufacturer
      .entry(manufacturer_name)
      .or_default();

    for (name, link) in aircraft {
      aircraft_map.insert(name.into(), link.into());
    }

    write_json(&self.path, &*aircraft_by_manufacturer).await
  }

  pub async fn snapshot(&self) -> AircraftByManufacturerMap {
    self.aircraft_by_manufacturer.lock().await.clone()
  }
}

#[derive(Clone, Debug)]
pub struct PlaneStore {
  path: PathBuf,
  planes_by_manufacturer: Arc<Mutex<PlaneByManufacturerMap>>,
}

impl PlaneStore {
  pub async fn load(path: impl Into<PathBuf>) -> Result<Self> {
    let path = path.into();
    let planes_by_manufacturer = read_json(&path).await?;

    Ok(Self {
      path,
      planes_by_manufacturer: Arc::new(Mutex::new(planes_by_manufacturer)),
    })
  }

  /// A store backed by a path nothing writes to, for tests that need a spider
  /// but never save an item.
  #[cfg(test)]
  pub fn empty_for_test() -> Self {
    Self {
      path: PathBuf::from("/dev/null"),
      planes_by_manufacturer: Arc::new(Mutex::new(PlaneByManufacturerMap::new())),
    }
  }

  /// Stores a scraped record without ever making the stored one poorer.
  ///
  /// A plain replace loses data whenever a re-scrape returns less than the file
  /// already holds, and that is not hypothetical: PlanePHD's `/research/<slug>`
  /// layout puts the ownership-cost breakdown behind a sign-in, so every record
  /// re-read from it has an empty `ownership_costs`. Eleven aircraft that shared
  /// a detail page with a missing listing were rewritten that way and lost cost
  /// figures that had been scraped from the old layout.
  pub async fn insert(&self, item: PlaneItem) -> Result<()> {
    let mut planes_by_manufacturer = self.planes_by_manufacturer.lock().await;

    let slot = planes_by_manufacturer
      .entry(item.manufacturer_name.clone())
      .or_default()
      .entry(item.aircraft_name.clone());

    match slot {
      std::collections::btree_map::Entry::Vacant(vacant) => {
        vacant.insert(item);
      }
      std::collections::btree_map::Entry::Occupied(mut occupied) => {
        occupied.insert(merge_preferring_richer(occupied.get(), item));
      }
    }

    write_json(&self.path, &*planes_by_manufacturer).await
  }

  pub async fn insert_many<I>(&self, items: I) -> Result<()>
  where
    I: IntoIterator<Item = PlaneItem>,
  {
    let mut planes_by_manufacturer = self.planes_by_manufacturer.lock().await;

    for item in items {
      planes_by_manufacturer
        .entry(item.manufacturer_name.clone())
        .or_default()
        .insert(item.aircraft_name.clone(), item);
    }

    write_json(&self.path, &*planes_by_manufacturer).await
  }

  pub async fn snapshot(&self) -> PlaneByManufacturerMap {
    self.planes_by_manufacturer.lock().await.clone()
  }
}

async fn read_json<T>(path: impl AsRef<Path>) -> Result<T>
where
  T: Default + serde::de::DeserializeOwned,
{
  let path = path.as_ref();

  match fs::read_to_string(path).await {
    Ok(contents) if contents.trim().is_empty() => Ok(T::default()),
    Ok(contents) => Ok(serde_json::from_str(&contents)?),
    Err(error) if error.kind() == ErrorKind::NotFound => Ok(T::default()),
    Err(error) => Err(error.into()),
  }
}

/// Keeps whichever side actually has the value, field by field.
///
/// The incoming record wins wherever it says something, because it is the newer
/// reading; the stored record is consulted for every key the incoming one does
/// not mention. That makes a re-scrape monotonic -- it can add and correct,
/// never subtract -- which is the property a partial layout change would
/// otherwise break silently. The cost is that a measurement the source genuinely
/// withdraws stays until a reading overwrites it; against a source now
/// publishing strictly less, that is the trade worth taking.
fn merge_preferring_richer(stored: &PlaneItem, incoming: PlaneItem) -> PlaneItem {
  fn richer(
    incoming: BTreeMap<String, String>,
    stored: &BTreeMap<String, String>,
  ) -> BTreeMap<String, String> {
    // Union, keyed: the newer reading wins on a key both sides have, and a key
    // only the stored side has survives. Whole-section replacement was too
    // coarse -- the `/research` layout publishes five or so performance figures
    // where the legacy one published up to fifteen, so a non-empty-but-thinner
    // section still silently dropped measurements.
    let mut merged = stored.clone();
    merged.extend(incoming);
    merged
  }

  PlaneItem {
    title: incoming.title.or_else(|| stored.title.clone()),
    description: incoming.description.or_else(|| stored.description.clone()),
    papi_price_estimate: incoming
      .papi_price_estimate
      .or_else(|| stored.papi_price_estimate.clone()),
    for_sale_count: incoming
      .for_sale_count
      .or_else(|| stored.for_sale_count.clone()),
    performance: richer(incoming.performance, &stored.performance),
    weights: richer(incoming.weights, &stored.weights),
    ownership_costs: richer(incoming.ownership_costs, &stored.ownership_costs),
    engine: richer(incoming.engine, &stored.engine),
    images: if incoming.images.is_empty() {
      stored.images.clone()
    } else {
      incoming.images
    },
    ..incoming
  }
}

async fn write_json<T>(path: impl AsRef<Path>, value: &T) -> Result<()>
where
  T: serde::Serialize,
{
  let path = path.as_ref();

  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent).await?;
  }

  let contents = serde_json::to_string_pretty(value)?;
  let temp_path = temporary_path(path);

  fs::write(&temp_path, contents).await?;
  fs::rename(&temp_path, path).await?;

  Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
  let file_name = path
    .file_name()
    .and_then(|value| value.to_str())
    .unwrap_or("output.json");

  let process_id = std::process::id();

  let timestamp = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |duration| duration.as_nanos());

  path.with_file_name(format!("{file_name}.{process_id}.{timestamp}.tmp"))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn item(costs: &[(&str, &str)], cruise: Option<&str>) -> PlaneItem {
    PlaneItem {
      manufacturer_name: String::from("HAWKER"),
      aircraft_name: String::from("800XP (1995 - 2005)"),
      source_link: String::new(),
      page_url: String::new(),
      title: None,
      description: None,
      papi_price_estimate: None,
      for_sale_count: None,
      performance: cruise
        .map(|v| BTreeMap::from([(String::from("best_cruise_speed"), String::from(v))]))
        .unwrap_or_default(),
      weights: BTreeMap::new(),
      ownership_costs: costs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect(),
      engine: BTreeMap::new(),
      images: Vec::new(),
    }
  }

  #[test]
  fn a_rescrape_that_returns_less_does_not_erase_what_is_stored() {
    // The exact shape that cost eleven aircraft their cost figures: the new
    // layout carries specs but no ownership_costs, and it shares a detail page
    // with a listing that still had to be fetched.
    let stored = item(&[("insurance", "$53,510.41")], Some("447 KIAS"));
    let thinner = item(&[], Some("448 kt"));

    let merged = merge_preferring_richer(&stored, thinner);

    assert_eq!(
      merged.ownership_costs.get("insurance").map(String::as_str),
      Some("$53,510.41"),
      "an empty section must not replace a populated one"
    );
    assert_eq!(
      merged
        .performance
        .get("best_cruise_speed")
        .map(String::as_str),
      Some("448 kt"),
      "but where the new reading says something, it is the newer reading that wins"
    );
  }

  #[test]
  fn a_rescrape_with_fewer_keys_in_a_section_keeps_the_ones_it_omits() {
    let mut stored = item(&[], Some("447 KIAS"));
    stored
      .performance
      .insert(String::from("ceiling"), String::from("41,000 FT"));
    let thinner = item(&[], Some("448 kt"));

    let merged = merge_preferring_richer(&stored, thinner);

    assert_eq!(
      merged.performance.get("ceiling").map(String::as_str),
      Some("41,000 FT"),
      "a section that is non-empty but thinner must not drop the keys it lacks"
    );
    assert_eq!(
      merged
        .performance
        .get("best_cruise_speed")
        .map(String::as_str),
      Some("448 kt")
    );
  }

  #[test]
  fn a_rescrape_that_returns_more_replaces_what_is_stored() {
    let stored = item(&[], None);
    let richer = item(&[("insurance", "$1.00")], Some("100 kt"));

    let merged = merge_preferring_richer(&stored, richer);

    assert_eq!(
      merged.ownership_costs.len(),
      1,
      "a populated section must land"
    );
    assert_eq!(merged.performance.len(), 1);
  }
}
