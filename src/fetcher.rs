use crate::prelude::*;
use rand::Rng;

use reqwest::{
  Client,
  header::{ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, HeaderMap, HeaderValue},
};
use std::{env, time::Duration};

#[derive(Clone)]
pub struct Fetcher {
  client: Client,
}

impl Fetcher {
  pub fn new(config: FetcherConfig) -> Result<Self> {
    const USER_AGENTS: &[&str] = &[
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/92.0.4515.107 Safari/537.36",
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.114 Safari/537.36",
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:94.0) Gecko/20100101 Firefox/94.0",
      "Mozilla/5.0 (iPhone; CPU iPhone OS 15_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/15.0 Mobile/15E148 Safari/604.1",
    ];
    let mut headers = HeaderMap::new();
    let user_agent = USER_AGENTS[rand::rng().random_range(0..USER_AGENTS.len())];

    headers.insert("User-Agent", HeaderValue::from_str(user_agent).unwrap());
    headers.insert(
      ACCEPT,
      HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
    );
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));

    if let Some(token) = config.bearer_token {
      let value = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|err| Error::Internal(format!("invalid authorization header: {err}")))?;

      headers.insert(AUTHORIZATION, value);
    }

    let client = Client::builder()
      // .user_agent(config.user_agent)
      .default_headers(headers)
      .timeout(config.timeout)
      .build()?;

    Ok(Self { client })
  }

  pub async fn get_text(&self, url: &str) -> Result<String> {
    let response = self.client.get(url).send().await?.error_for_status()?;
    let body = response.text().await?;

    if body.trim().is_empty() {
      return Err(Error::Internal(format!("empty response body for {url}")));
    }

    Ok(body)
  }
}

pub struct FetcherConfig {
  // user_agent: String,
  timeout: Duration,
  bearer_token: Option<String>,
}

impl FetcherConfig {
  pub fn from_env() -> Self {
    Self {
      // user_agent: env::var("CRAWLER_USER_AGENT")
      //     .unwrap_or_else(|_| APP_USER_AGENT.to_string()),
      timeout: Duration::from_secs(15),
      bearer_token: env::var("ACCESS_TOKEN").ok(),
    }
  }
}
