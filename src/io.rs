use crate::prelude::*;
use std::{
  collections::BTreeMap,
  io::ErrorKind,
  path::{Path, PathBuf},
  sync::Arc,
};
use tokio::{fs, sync::Mutex};

pub type ManufacturerMap = BTreeMap<String, String>;

#[derive(Clone, Debug)]
pub struct ManufacturerStore {
  path: PathBuf,
  manufacturers: Arc<Mutex<ManufacturerMap>>,
}

impl ManufacturerStore {
  pub async fn load(path: impl Into<PathBuf>) -> Result<Self> {
    let path = path.into();
    let manufacturers = read_manufacturers(&path).await?;

    Ok(Self {
      path,
      manufacturers: Arc::new(Mutex::new(manufacturers)),
    })
  }

  pub async fn insert(&self, name: impl Into<String>, link: impl Into<String>) -> Result<()> {
    let mut manufacturers = self.manufacturers.lock().await;

    manufacturers.insert(name.into(), link.into());
    write_manufacturers(&self.path, &manufacturers).await
  }
}

async fn read_manufacturers(path: impl AsRef<Path>) -> Result<ManufacturerMap> {
  let path = path.as_ref();

  match fs::read_to_string(path).await {
    Ok(contents) if contents.trim().is_empty() => Ok(ManufacturerMap::new()),
    Ok(contents) => Ok(serde_json::from_str(&contents)?),
    Err(error) if error.kind() == ErrorKind::NotFound => Ok(ManufacturerMap::new()),
    Err(error) => Err(error.into()),
  }
}

async fn write_manufacturers(
  path: impl AsRef<Path>,
  manufacturers: &ManufacturerMap,
) -> Result<()> {
  let path = path.as_ref();

  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent).await?;
  }

  let contents = serde_json::to_string_pretty(manufacturers)?;
  let temp_path = temporary_path(path);

  fs::write(&temp_path, contents).await?;
  fs::rename(&temp_path, path).await?;

  Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
  let mut temp_path = path.to_path_buf();

  let extension = path
    .extension()
    .and_then(|value| value.to_str())
    .map_or_else(|| "tmp".to_string(), |value| format!("{value}.tmp"));

  temp_path.set_extension(extension);
  temp_path
}
