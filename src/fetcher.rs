use crate::prelude::*;
use rand::Rng;
use reqwest::{
  Client, StatusCode,
  header::{
    ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, CONNECTION, HeaderMap, HeaderValue, RETRY_AFTER,
    USER_AGENT,
  },
};
use std::{env, time::Duration};
use tokio::time::sleep;

const USER_AGENTS: &[&str] = &[
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/92.0.4515.107 Safari/537.36",
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.114 Safari/537.36",
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:94.0) Gecko/20100101 Firefox/94.0",
  "Mozilla/5.0 (iPhone; CPU iPhone OS 15_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/15.0 Mobile/15E148 Safari/604.1",
];

#[derive(Clone)]
pub struct Fetcher {
  client: Client,
  config: FetcherConfig,
}

impl Fetcher {
  pub fn new(config: FetcherConfig) -> Result<Self> {
    let mut headers = HeaderMap::new();

    headers.insert(
      USER_AGENT,
      HeaderValue::from_str(&config.user_agent)
        .map_err(|err| Error::Internal(format!("invalid user-agent header: {err}")))?,
    );

    headers.insert(
      ACCEPT,
      HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
    );

    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));

    headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));

    if let Some(token) = config.bearer_token.as_deref() {
      let value = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|err| Error::Internal(format!("invalid authorization header: {err}")))?;

      headers.insert(AUTHORIZATION, value);
    }

    let client = Client::builder()
      .default_headers(headers)
      .timeout(config.timeout)
      .build()?;

    Ok(Self { client, config })
  }

  pub async fn get_text(&self, url: &str) -> Result<String> {
    let mut attempt = 0;
    let max_attempts = self.config.max_retries.saturating_add(1);

    loop {
      attempt += 1;

      log::debug!("fetch attempt {attempt}/{max_attempts}: {url}");

      let response = match self.client.get(url).send().await {
        Ok(response) => response,
        Err(error) => {
          if attempt >= max_attempts || !is_retryable_reqwest_error(&error) {
            return Err(error.into());
          }

          let retry_delay = self.exponential_backoff_with_jitter(attempt);

          log::warn!(
            "request failed: url={url}, attempt={attempt}/{max_attempts}, error={error}, sleeping={}s",
            retry_delay.as_secs()
          );

          sleep(retry_delay).await;
          continue;
        }
      };

      let status = response.status();

      log::debug!("fetch status: status={status}, url={url}");

      match status {
        StatusCode::OK => {
          let body = response.text().await?;

          if body.trim().is_empty() {
            return Err(Error::Internal(format!("empty response body for {url}")));
          }

          return Ok(body);
        }

        StatusCode::TOO_MANY_REQUESTS => {
          if attempt >= max_attempts {
            return Err(Error::RateLimited {
              url: url.to_string(),
              attempts: attempt,
            });
          }

          let retry_delay = retry_delay_from_headers(response.headers())
            .unwrap_or_else(|| self.exponential_backoff_with_jitter(attempt));

          let retry_delay = self.cap_retry_delay(retry_delay);

          log::warn!(
            "rate limited: url={url}, attempt={attempt}/{max_attempts}, sleeping={}s",
            retry_delay.as_secs()
          );

          sleep(retry_delay).await;
        }

        status if status.is_server_error() => {
          if attempt >= max_attempts {
            return Err(Error::HttpStatus {
              url: url.to_string(),
              status,
            });
          }

          let retry_delay = self.exponential_backoff_with_jitter(attempt);

          log::warn!(
            "server error: status={status}, url={url}, attempt={attempt}/{max_attempts}, sleeping={}s",
            retry_delay.as_secs()
          );

          sleep(retry_delay).await;
        }

        StatusCode::NOT_FOUND => {
          return Err(Error::AircraftNotFound {
            url: url.to_string(),
          });
        }

        status => {
          return Err(Error::HttpStatus {
            url: url.to_string(),
            status,
          });
        }
      }
    }
  }

  fn exponential_backoff_with_jitter(&self, attempt: usize) -> Duration {
    let exponent = attempt.saturating_sub(1).min(6);
    let multiplier = 2_u64.pow(exponent as u32);

    let base_delay = self
      .config
      .base_retry_delay
      .as_secs()
      .saturating_mul(multiplier);

    let capped_delay = base_delay.min(self.config.max_retry_delay.as_secs());
    let jitter = rand::rng().random_range(0..=1);

    Duration::from_secs(capped_delay + jitter)
  }

  fn cap_retry_delay(&self, delay: Duration) -> Duration {
    delay.min(self.config.max_retry_delay)
  }
}

#[derive(Clone)]
pub struct FetcherConfig {
  pub user_agent: String,
  pub timeout: Duration,
  pub bearer_token: Option<String>,
  pub max_retries: usize,
  pub base_retry_delay: Duration,
  pub max_retry_delay: Duration,
}

impl FetcherConfig {
  pub fn from_env() -> Self {
    Self {
      user_agent: USER_AGENTS[rand::rng().random_range(0..USER_AGENTS.len())].to_string(),
      timeout: Duration::from_secs(15),
      bearer_token: env::var("ACCESS_TOKEN").ok(),
      max_retries: 3,
      base_retry_delay: Duration::from_secs(2),
      max_retry_delay: Duration::from_secs(30),
    }
  }
}

fn retry_delay_from_headers(headers: &HeaderMap) -> Option<Duration> {
  let value = headers.get(RETRY_AFTER)?.to_str().ok()?;
  let seconds = value.parse::<u64>().ok()?;

  Some(Duration::from_secs(seconds))
}

fn is_retryable_reqwest_error(error: &reqwest::Error) -> bool {
  error.is_timeout() || error.is_connect() || error.is_request()
}
