// https://planephd.com/wizard/?modeltype=PistonSingle&min_year=1900&required_seats_min=4&ownership_cost_p_year_max=50000&purchase_price_max=1000000&min_speed=120&annual_hrs=100
use crate::error::Error;
use async_trait::async_trait;
use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub struct PlanePhdSpider {
    http_client: Client,
}

impl PlanePhdSpider {
    pub fn new() -> Self {
        let http_timeout = Duration::from_secs(6);
        let mut headers = header::HeaderMap::new();
        headers.insert(
            "Accept",
            header::HeaderValue::from_static("application/json"),
        );

        let http_client = Client::builder()
            .timeout(http_timeout)
            .default_headers(headers)
            .user_agent(
                "Mozilla/5.0 (Windows NT 6.1; Win64; x64; rv:47.0) Gecko/20100101 Firefox/47.0",
            )
            .build()
            .expect("spiders/github: Building HTTP client");

        PlanePhdSpider {
            http_client,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaneItem {
    name: String,
    age: String,
}

#[async_trait]
impl super::Spider for PlanePhdSpider {
    type Item = PlaneItem;

    fn name(&self) -> String {
        String::from("github")
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://planephd.com/wizard/?modeltype=PistonSingle&min_year=1900&required_seats_min=4&ownership_cost_p_year_max=50000&purchase_price_max=1000000&min_speed=120&annual_hrs=100".to_string()]
    }

    async fn scrape(&self, url: String) -> Result<Vec<PlaneItem>, Error> {
        let items: Vec<PlaneItem> = self.http_client.get(&url).send().await?.json().await?;



        Ok((items))
    }

    async fn process(&self, item: Self::Item) -> Result<(), Error> {
        println!("{}, {}", item.name, item.age);

        Ok(())
    }
}
