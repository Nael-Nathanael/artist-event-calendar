use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use uuid::Uuid;

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
    pub event_artists: Vec<(String, String)>,
}

impl IngestPayload {
    fn merge(&mut self, other: IngestPayload) {
        self.series.extend(other.series);
        self.events.extend(other.events);
        self.days.extend(other.days);
        self.artists.extend(other.artists);
        self.event_artists.extend(other.event_artists);
    }
}

// --- Scraper Trait ---
#[async_trait]
pub trait ScraperBase: Send + Sync {
    async fn scrape(&self, client: &Client) -> Result<IngestPayload, Box<dyn std::error::Error>>;
    fn name(&self) -> &'static str;
}

// --- Scraper Implementations ---

/// Comifuro Scraper (Mock implementation representing parsing comifuro.net)
struct ComifuroScraper;
#[async_trait]
impl ScraperBase for ComifuroScraper {
    fn name(&self) -> &'static str { "Comifuro" }
    
    async fn scrape(&self, _client: &Client) -> Result<IngestPayload, Box<dyn std::error::Error>> {
        // Here we would use `client.get("https://comifuro.net/").send().await?`
        // and parse it using `scraper::Html::parse_document(&text)`
        // For demonstration, we'll return a dynamically generated payload using the scraper's namespace.
        
        let event_url = "https://comifuro.net";
        let event_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, event_url.as_bytes()).to_string();

        let event = Event {
            id: event_id.clone(),
            series_id: None,
            title: "Comic Frontier 19".to_string(),
            category: "Convention".to_string(),
            location_name: Some("ICE BSD".to_string()),
            location_city: Some("Tangerang".to_string()),
            floorplan_image_url: None,
            banner_image_url: None,
            official_url: Some(event_url.to_string()),
            start_date: "2026-11-09T00:00:00Z".to_string(), // Future date for calendar
            end_date: "2026-11-10T23:59:59Z".to_string(),
        };

        let mut payload = IngestPayload::default();
        payload.events.push(event);
        
        Ok(payload)
    }
}

/// Pestapora Scraper
struct PestaporaScraper;
#[async_trait]
impl ScraperBase for PestaporaScraper {
    fn name(&self) -> &'static str { "Pestapora" }
    
    async fn scrape(&self, _client: &Client) -> Result<IngestPayload, Box<dyn std::error::Error>> {
        let event_url = "https://pestapora.com/2026";
        let event_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, event_url.as_bytes()).to_string();

        let event = Event {
            id: event_id.clone(),
            series_id: None,
            title: "Pestapora 2026".to_string(),
            category: "MusicFestival".to_string(),
            location_name: Some("Gambir Expo".to_string()),
            location_city: Some("Jakarta".to_string()),
            floorplan_image_url: None,
            banner_image_url: None,
            official_url: Some("https://pestapora.com".to_string()),
            start_date: "2026-10-25T00:00:00Z".to_string(),
            end_date: "2026-10-27T23:59:59Z".to_string(),
        };

        // Add some artists
        let artist_names = ["Hindia", "Tulus", "Maliq & D'Essentials"];
        let mut artists = Vec::new();
        let mut event_artists = Vec::new();

        for name in artist_names {
            let artist_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("artist:{}", name).as_bytes()).to_string();
            artists.push(Artist {
                id: artist_id.clone(),
                name: name.to_string(),
                profile_image_url: None,
            });
            event_artists.push((event_id.clone(), artist_id));
        }

        let mut payload = IngestPayload::default();
        payload.events.push(event);
        payload.artists = artists;
        payload.event_artists = event_artists;
        
        Ok(payload)
    }
}

// --- Main Worker Loop ---

async fn run_all_scrapers(client: &Client) -> Result<IngestPayload, Box<dyn std::error::Error>> {
    let scrapers: Vec<Box<dyn ScraperBase>> = vec![
        Box::new(ComifuroScraper),
        Box::new(PestaporaScraper),
        Box::new(scrapers::punipun::PunipunScraper),
    ];

    let mut master_payload = IngestPayload::default();

    for scraper in scrapers {
        tracing::info!("Running scraper: {}", scraper.name());
        match scraper.scrape(client).await {
            Ok(payload) => {
                tracing::info!("{} returned {} events", scraper.name(), payload.events.len());
                master_payload.merge(payload);
            }
            Err(e) => {
                tracing::error!("Scraper {} failed: {}", scraper.name(), e);
            }
        }
    }

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

    loop {
        tracing::info!("Starting scraper batch run...");
        match run_all_scrapers(&client).await {
            Ok(payload) => {
                tracing::info!("Batch scraping complete. Sending {} total events to backend...", payload.events.len());
                let res = client.post(&format!("{}/api/internal/ingest/batch", backend_url))
                    .bearer_auth(&api_key)
                    .json(&payload)
                    .send()
                    .await;
                
                match res {
                    Ok(r) if r.status().is_success() => {
                        tracing::info!("Successfully ingested data into API.");
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
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}
