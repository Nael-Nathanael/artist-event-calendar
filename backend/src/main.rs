use axum::{
    extract::{State},
    http::{StatusCode, HeaderMap},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::{SqliteConnectOptions, SqlitePoolOptions}, SqlitePool};
use std::{env, net::SocketAddr, sync::Arc, str::FromStr};
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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

#[derive(Debug, Serialize, Deserialize)]
pub struct IngestPayload {
    pub series: Vec<EventSeries>,
    pub events: Vec<Event>,
    pub days: Vec<EventDay>,
    pub artists: Vec<Artist>,
    pub event_artists: Vec<(String, String)>, // event_id, artist_id
}

// --- App State ---

struct AppState {
    db: SqlitePool,
    api_key: String,
}

use axum::extract::Query;

#[derive(Deserialize)]
struct EventParams {
    from: Option<String>,
    to: Option<String>,
}

async fn get_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EventParams>,
) -> Result<Json<Vec<Event>>, (StatusCode, String)> {
    let from_date = params.from.unwrap_or_else(|| "1970-01-01".to_string());
    let to_date = params.to.unwrap_or_else(|| "9999-12-31".to_string());

    let events = sqlx::query_as!(
        Event,
        r#"
        SELECT id, series_id, title, category, location_name, location_city, floorplan_image_url, banner_image_url, official_url, start_date, end_date
        FROM events
        WHERE start_date < ? AND end_date >= ?
        ORDER BY start_date ASC
        "#,
        to_date,
        from_date
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch events".to_string())
    })?;

    Ok(Json(events))
}

async fn ingest_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<IngestPayload>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let auth_header = headers.get("Authorization").and_then(|h| h.to_str().ok());
    if auth_header != Some(&format!("Bearer {}", state.api_key)) {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string()));
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    // Upsert Series
    for s in payload.series {
        sqlx::query!(
            r#"
            INSERT INTO event_series (id, name, description)
            VALUES (?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description
            "#,
            s.id, s.name, s.description
        ).execute(&mut *tx).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // Upsert Events
    for e in payload.events {
        sqlx::query!(
            r#"
            INSERT INTO events (id, series_id, title, category, location_name, location_city, floorplan_image_url, banner_image_url, official_url, start_date, end_date)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET 
                series_id=excluded.series_id, title=excluded.title, category=excluded.category,
                location_name=excluded.location_name, location_city=excluded.location_city,
                floorplan_image_url=excluded.floorplan_image_url, banner_image_url=excluded.banner_image_url,
                official_url=excluded.official_url, start_date=excluded.start_date, end_date=excluded.end_date
            "#,
            e.id, e.series_id, e.title, e.category, e.location_name, e.location_city, e.floorplan_image_url, e.banner_image_url, e.official_url, e.start_date, e.end_date
        ).execute(&mut *tx).await.map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    }

    // Upsert Days
    for d in payload.days {
        sqlx::query!(
            r#"
            INSERT INTO event_days (id, event_id, name, date, rundown_json)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET name=excluded.name, date=excluded.date, rundown_json=excluded.rundown_json
            "#,
            d.id, d.event_id, d.name, d.date, d.rundown_json
        ).execute(&mut *tx).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // Upsert Artists
    for a in payload.artists {
        sqlx::query!(
            r#"
            INSERT INTO artists (id, name, profile_image_url)
            VALUES (?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET name=excluded.name, profile_image_url=excluded.profile_image_url
            "#,
            a.id, a.name, a.profile_image_url
        ).execute(&mut *tx).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // Insert Event-Artists (ignore conflicts)
    for (event_id, artist_id) in payload.event_artists {
        sqlx::query!(
            r#"
            INSERT INTO event_artists (event_id, artist_id)
            VALUES (?, ?)
            ON CONFLICT(event_id, artist_id) DO NOTHING
            "#,
            event_id, artist_id
        ).execute(&mut *tx).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    tx.commit().await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    Ok((StatusCode::OK, "Batch ingested successfully"))
}


// --- Main ---

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "backend=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://events.db".to_string());
    let api_key = env::var("API_KEY").unwrap_or_else(|_| "dev-secret-key".to_string());
    
    let opts = SqliteConnectOptions::from_str(&db_url)?.create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;

    let schema = std::fs::read_to_string("schema.sql").unwrap_or_else(|_| "".to_string());
    if !schema.is_empty() {
        tracing::info!("Initializing database schema...");
        let mut conn = pool.acquire().await?;
        sqlx::query(&schema).execute(&mut *conn).await?;
    }

    let state = Arc::new(AppState { db: pool, api_key });

    let app = Router::new()
        .route("/api/events", get(get_events))
        .route("/api/internal/ingest/batch", post(ingest_batch))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8081));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
