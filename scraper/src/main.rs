use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::time::{Duration, Instant};

mod dedupe;
mod scrapers;

// --- Models ---
#[derive(Debug, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub series_id: Option<String>,
    pub title: String,
    pub category: String,
    pub location_name: Option<String>,
    pub location_city: Option<String>,
    pub floorplan_image_url: Option<String>,
    pub banner_image_url: Option<String>,
    pub official_url: Option<String>,
    pub start_date: String,
    pub end_date: String,
    pub description: Option<String>,
    pub organizer: Option<String>,
    /// Name of the scraper that produced it; `run_all_scrapers` fills it in.
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventArtist {
    pub event_id: String,
    pub artist_id: String,
    /// What the artist does there, e.g. "Guest Cosplayer".
    pub role: Option<String>,
    /// Name of the scraper that produced it; `run_all_scrapers` fills it in.
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventDay {
    pub id: String,
    pub event_id: String,
    pub name: String,
    pub date: String,
    pub rundown_json: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub profile_image_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventSeries {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct IngestPayload {
    pub series: Vec<EventSeries>,
    pub events: Vec<Event>,
    pub days: Vec<EventDay>,
    pub artists: Vec<Artist>,
    pub event_artists: Vec<EventArtist>,
    /// Scrapers that succeeded this run. The backend treats their events and
    /// line-ups in this batch as complete and deletes their rows missing from it.
    pub sources: Vec<String>,
}

impl IngestPayload {
    fn merge(&mut self, source: &str, mut other: IngestPayload) {
        other.events.iter_mut().for_each(|e| e.source = source.to_string());
        other.event_artists.iter_mut().for_each(|l| l.source = source.to_string());
        self.series.extend(other.series);
        self.events.extend(other.events);
        self.days.extend(other.days);
        self.artists.extend(other.artists);
        self.event_artists.extend(other.event_artists);
        self.sources.push(source.to_string());
    }
}

// --- Scraper Trait ---
#[async_trait]
pub trait ScraperBase: Send + Sync {
    async fn scrape(&self, client: &Client) -> Result<IngestPayload, Box<dyn std::error::Error>>;
    fn name(&self) -> &'static str;
    /// Minimum time between successful runs; zero runs it every batch.
    fn interval(&self) -> Duration {
        Duration::ZERO
    }
}

// --- Main Worker Loop ---

/// Runs every scraper that is due. A source skipped here is left out of `sources`,
/// so the backend keeps its rows from the last run.
async fn run_all_scrapers(
    client: &Client,
    scrapers: &[Box<dyn ScraperBase>],
    last_ingested: &HashMap<&'static str, Instant>,
) -> Result<IngestPayload, Box<dyn std::error::Error>> {
    let mut master_payload = IngestPayload::default();

    for scraper in scrapers {
        if last_ingested.get(scraper.name()).is_some_and(|t| t.elapsed() < scraper.interval()) {
            tracing::info!("Skipping scraper {}: not due yet", scraper.name());
            continue;
        }
        tracing::info!("Running scraper: {}", scraper.name());
        match scraper.scrape(client).await {
            Ok(payload) => {
                tracing::info!("{} returned {} events", scraper.name(), payload.events.len());
                master_payload.merge(scraper.name(), payload);
            }
            Err(e) => {
                tracing::error!("Scraper {} failed: {}", scraper.name(), e);
            }
        }
    }

    dedupe::merge_duplicates(&mut master_payload);
    dedupe::attach_appearances(&mut master_payload);
    Ok(master_payload)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let backend_url = env::var("BACKEND_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".to_string());
    let api_key = env::var("API_KEY").unwrap_or_else(|_| "dev-secret-key".to_string());
    
    let client = Client::builder()
        .user_agent("ArtistEventCalendar/1.0")
        .build()?;

    let scrapers: Vec<Box<dyn ScraperBase>> = vec![
        Box::new(scrapers::punipun::PunipunScraper),
        Box::new(scrapers::jiexpo::JiexpoScraper),
        Box::new(scrapers::ruangcosplay::RuangCosplayScraper),
        Box::new(scrapers::eventfest::EventfestScraper),
        Box::new(scrapers::eventseye::EventseyeScraper),
        Box::new(scrapers::jmwf::JmwfScraper),
    ];
    // In memory: a restart runs every source once, which is also when new code ships.
    let mut last_ingested: HashMap<&'static str, Instant> = HashMap::new();

    loop {
        tracing::info!("Starting scraper batch run...");
        match run_all_scrapers(&client, &scrapers, &last_ingested).await {
            Ok(payload) => {
                tracing::info!("Batch scraping complete. Sending {} total events to backend...", payload.events.len());
                let res = client.post(format!("{}/api/internal/ingest/batch", backend_url))
                    .bearer_auth(&api_key)
                    .json(&payload)
                    .send()
                    .await;

                match res {
                    Ok(r) if r.status().is_success() => {
                        tracing::info!("Successfully ingested data into API.");
                        for s in scrapers.iter().filter(|s| payload.sources.iter().any(|n| n == s.name())) {
                            last_ingested.insert(s.name(), Instant::now());
                        }
                    }
                    Ok(r) => {
                        let status = r.status();
                        let text = r.text().await.unwrap_or_default();
                        tracing::error!("Failed to ingest API: {} - {}", status, text);
                    }
                    Err(e) => {
                        tracing::error!("Network error pushing to backend: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Batch scraping failed: {}", e);
            }
        }
        
        // Run every hour
        tracing::info!("Sleeping for 1 hour...");
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}
