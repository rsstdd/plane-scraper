use reqwest::StatusCode;
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

  #[error("HTTP status error: {status} for {url}")]
  HttpStatus { url: String, status: StatusCode },

  #[error("Rate limited after {attempts} attempts: {url}")]
  RateLimited { url: String, attempts: usize },

  #[error("Aircraft detail page not found: {url}")]
  AircraftNotFound { url: String },

  #[error("I/O operation failed: {0}")]
  Io(#[from] std::io::Error),

  #[error("JSON operation failed: {0}")]
  Json(#[from] serde_json::Error),

  #[error("WebDriver: {0}")]
  WebDriver(String),
}
