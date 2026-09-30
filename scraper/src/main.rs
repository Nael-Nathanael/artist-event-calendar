use serde::{Deserialize, Serialize};
use std::env;
use uuid::Uuid;

// --- Models ---
// These should match the backend models closely.
#[derive(Debug, Serialize, Deserialize)]
pub struct EventSeries {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

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
pub struct IngestPayload {
    pub series: Vec<EventSeries>,
    pub events: Vec<Event>,
    pub days: Vec<EventDay>,
    pub artists: Vec<Artist>,
    pub event_artists: Vec<(String, String)>,
}

// Dummy Scraper logic to demonstrate structure
async fn run_scrapers() -> Result<IngestPayload, Box<dyn std::error::Error>> {
    tracing::info!("Running scrapers...");
    
    // Use a fixed namespace for deterministic UUIDs
    let namespace = Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();

    let artist_name = "Punipun";
    let artist_id = Uuid::new_v5(&namespace, artist_name.as_bytes()).to_string();
    let artist = Artist {
        id: artist_id.clone(),
        name: artist_name.to_string(),
        profile_image_url: Some("https://example.com/punipun.jpg".to_string()),
    };

    let event_name = "Punipun Meet & Greet Jakarta 2026";
    let event_id = Uuid::new_v5(&namespace, event_name.as_bytes()).to_string();
    let event = Event {
        id: event_id.clone(),
        series_id: None,
        title: event_name.to_string(),
        category: "MeetAndGreet".to_string(),
        location_name: Some("Mall of Indonesia".to_string()),
        location_city: Some("Jakarta".to_string()),
        floorplan_image_url: None,
        banner_image_url: None,
        official_url: Some("https://example.com/punipun-event".to_string()),
        start_date: "2026-10-15T00:00:00Z".to_string(),
        end_date: "2026-10-15T23:59:59Z".to_string(),
    };

    let payload = IngestPayload {
        series: vec![],
        events: vec![event],
        days: vec![],
        artists: vec![artist],
        event_artists: vec![(event_id, artist_id)],
    };

    Ok(payload)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let backend_url = env::var("BACKEND_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".to_string());
    let api_key = env::var("API_KEY").unwrap_or_else(|_| "dev-secret-key".to_string());
    
    loop {
        match run_scrapers().await {
            Ok(payload) => {
                tracing::info!("Scraping complete, sending {} events to backend...", payload.events.len());
                let client = reqwest::Client::new();
                let res = client.post(&format!("{}/api/internal/ingest/batch", backend_url))
                    .bearer_auth(&api_key)
                    .json(&payload)
                    .send()
                    .await;
                
                match res {
                    Ok(r) if r.status().is_success() => {
                        tracing::info!("Successfully ingested data.");
                    }
                    Ok(r) => {
                        tracing::error!("Failed to ingest: {:?}", r.status());
                    }
                    Err(e) => {
                        tracing::error!("Network error pushing to backend: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Scraper failed: {}", e);
            }
        }
        
        // Run every hour
        tracing::info!("Sleeping for 1 hour...");
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}
