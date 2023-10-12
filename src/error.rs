use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum Error {
  /// For starter, to remove as code matures.
  #[error("Generic error: {0}")]
  Generic(String),
  #[error("Internal")]
  Internal(String),
  #[error("Spider is not valid: {0}")]
  InvalidSpider(String),
  #[error("Reqwest: {0}")]
  Reqwest(String),
  #[error("WebDriver: {0}")]
  WebDriver(String),
}

impl std::convert::From<reqwest::Error> for Error {
  fn from(err: reqwest::Error) -> Self {
    Error::Reqwest(err.to_string())
  }
}
