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

  pub async fn insert(&self, item: PlaneItem) -> Result<()> {
    let mut planes_by_manufacturer = self.planes_by_manufacturer.lock().await;

    planes_by_manufacturer
      .entry(item.manufacturer_name.clone())
      .or_default()
      .insert(item.aircraft_name.clone(), item);

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
