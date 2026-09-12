use crate::spiders::Spider;
use futures::stream::StreamExt;
use serde::Serialize;
use std::{
  collections::HashSet,
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  },
  time::Duration,
};
use tokio::{
  sync::{Barrier, Mutex, mpsc},
  time::sleep,
};

/// Where a URL was lost, so a reader can tell "the site refused us" from "we
/// could not write the result".
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
  Scrape,
  Process,
}

#[derive(Clone, Debug, Serialize)]
pub struct FailedUrl {
  pub url: String,
  pub stage: Stage,
  pub error: String,
}

type Failures = Arc<Mutex<Vec<FailedUrl>>>;

pub struct Crawler {
  delay: Duration,
  crawling_concurrency: usize,
  processing_concurrency: usize,
  /// Where to write the URLs this run could not turn into records. Without it
  /// "the run finished" and "the run finished having scraped everything" are
  /// the same observation.
  failures_path: PathBuf,
}

impl Crawler {
  pub fn new(
    delay: Duration,
    crawling_concurrency: usize,
    processing_concurrency: usize,
    failures_path: PathBuf,
  ) -> Self {
    Crawler {
      delay,
      crawling_concurrency,
      processing_concurrency,
      failures_path,
    }
  }

  pub async fn run<T: Send + 'static>(&self, spider: Arc<dyn Spider<Item = T>>) {
    let mut visited_urls = HashSet::<String>::new();
    let crawling_concurrency = self.crawling_concurrency;
    let crawling_queue_capacity = crawling_concurrency * 400;
    let processing_concurrency = self.processing_concurrency;
    let processing_queue_capacity = processing_concurrency * 10;
    let active_spiders = Arc::new(AtomicUsize::new(0));
    let failures: Failures = Arc::new(Mutex::new(Vec::new()));

    let (urls_to_visit_tx, urls_to_visit_rx) = mpsc::channel(crawling_queue_capacity);
    let (items_tx, items_rx) = mpsc::channel(processing_queue_capacity);
    let (new_urls_tx, mut new_urls_rx) = mpsc::channel(crawling_queue_capacity);
    let barrier = Arc::new(Barrier::new(3));

    for url in spider.start_urls() {
      visited_urls.insert(url.clone());
      let _ = urls_to_visit_tx.send(url).await;
    }

    self.launch_processors(
      processing_concurrency,
      spider.clone(),
      items_rx,
      barrier.clone(),
      failures.clone(),
    );

    self.launch_scrapers(
      crawling_concurrency,
      spider.clone(),
      urls_to_visit_rx,
      new_urls_tx.clone(),
      items_tx,
      active_spiders.clone(),
      self.delay,
      barrier.clone(),
      failures.clone(),
    );

    loop {
      if let Some((visited_url, new_urls)) = new_urls_rx.try_recv().ok() {
        visited_urls.insert(visited_url);

        for url in new_urls {
          if !visited_urls.contains(&url) {
            visited_urls.insert(url.clone());
            log::debug!("queueing: {}", url);
            let _ = urls_to_visit_tx.send(url).await;
          }
        }
      }

      if new_urls_tx.capacity() == crawling_queue_capacity // new_urls channel is empty
            && urls_to_visit_tx.capacity() == crawling_queue_capacity // urls_to_visit channel is empty
            && active_spiders.load(Ordering::SeqCst) == 0
      {
        // no more work, we leave
        break;
      }

      sleep(Duration::from_millis(5)).await;
    }

    log::info!("crawler: control loop exited");

    // we drop the transmitter in order to close the stream
    drop(urls_to_visit_tx);

    // and then we wait for the streams to complete
    barrier.wait().await;

    self.write_failures(&failures).await;
  }

  /// Always written, empty array included: an absent file is ambiguous between
  /// "nothing failed" and "the run died before it could say".
  async fn write_failures(&self, failures: &Failures) {
    let failures = failures.lock().await;
    if !failures.is_empty() {
      log::warn!(
        "{} URL(s) produced no record; see {}",
        failures.len(),
        self.failures_path.display()
      );
    }
    match serde_json::to_vec_pretty(&*failures) {
      Ok(bytes) => {
        if let Err(err) = tokio::fs::write(&self.failures_path, bytes).await {
          log::error!("could not write {}: {err}", self.failures_path.display());
        }
      }
      Err(err) => log::error!("could not serialise the failure list: {err}"),
    }
  }

  fn launch_processors<T: Send + 'static>(
    &self,
    concurrency: usize,
    spider: Arc<dyn Spider<Item = T>>,
    items: mpsc::Receiver<T>,
    barrier: Arc<Barrier>,
    failures: Failures,
  ) {
    tokio::spawn(async move {
      tokio_stream::wrappers::ReceiverStream::new(items)
        .for_each_concurrent(concurrency, |item| {
          let failures = Arc::clone(&failures);
          let spider = Arc::clone(&spider);
          async move {
            // Discarding this result made a failed write to planes.json
            // indistinguishable from a successful one: the run ended "clean"
            // with records missing and nothing said which.
            if let Err(err) = spider.process(item).await {
              log::error!("failed to save a scraped item: {err}");
              failures.lock().await.push(FailedUrl {
                url: String::from("<item write>"),
                stage: Stage::Process,
                error: err.to_string(),
              });
            }
          }
        })
        .await;

      barrier.wait().await;
    });
  }

  fn launch_scrapers<T: Send + 'static>(
    &self,
    concurrency: usize,
    spider: Arc<dyn Spider<Item = T>>,
    urls_to_visit: mpsc::Receiver<String>,
    new_urls_tx: mpsc::Sender<(String, Vec<String>)>,
    items_tx: mpsc::Sender<T>,
    active_spiders: Arc<AtomicUsize>,
    delay: Duration,
    barrier: Arc<Barrier>,
    failures: Failures,
  ) {
    tokio::spawn(async move {
      tokio_stream::wrappers::ReceiverStream::new(urls_to_visit)
        .for_each_concurrent(concurrency, |queued_url| {
          let queued_url = queued_url.clone();
          async {
            active_spiders.fetch_add(1, Ordering::SeqCst);
            let mut urls = Vec::new();
            let res = match spider.scrape(queued_url.clone()).await {
              Ok(res) => Some(res),
              Err(err) => {
                // Logged *and* recorded. Logging alone is why the cause of the
                // 40 detail URLs that never produced a record is unrecoverable:
                // the run is over, the output says nothing, and the next run has
                // no worklist to start from.
                log::error!("{}", err);
                failures.lock().await.push(FailedUrl {
                  url: queued_url.clone(),
                  stage: Stage::Scrape,
                  error: err.to_string(),
                });
                None
              }
            };

            if let Some((items, new_urls)) = res {
              for item in items {
                let _ = items_tx.send(item).await;
              }
              urls = new_urls;
            }

            let _ = new_urls_tx.send((queued_url, urls)).await;
            sleep(delay).await;
            active_spiders.fetch_sub(1, Ordering::SeqCst);
          }
        })
        .await;

      drop(items_tx);
      barrier.wait().await;
    });
  }
}
