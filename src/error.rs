use thiserror::Error;


#[derive(Error, Debug)]
pub enum Error {
  /// Temporary fallback while the codebase matures.
  #[error("Generic error: {0}")]
  Generic(String),

  #[error("Internal error: {0}")]
  Internal(String),

  #[error("Invalid spider: {0}")]
  InvalidSpider(String),

  #[error("Request failed: {0}")]
  Reqwest(#[from] reqwest::Error),

  #[error("I/O operation failed: {0}")]
  Io(#[from] std::io::Error),

  #[error("WebDriver: {0}")]
  WebDriver(String),
}