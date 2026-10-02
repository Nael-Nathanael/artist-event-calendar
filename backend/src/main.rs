use axum::{
    extract::{State},
    http::{StatusCode, HeaderMap},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::{SqliteConnectOptions, SqlitePoolOptions}, SqlitePool};
use std::{collections::HashMap, env, net::SocketAddr, sync::Arc, str::FromStr};
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
    pub description: Option<String>,
    pub organizer: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct IngestEvent {
    #[serde(flatten)]
    pub event: Event,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct Lineup {
    pub name: String,
    pub role: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EventView {
    #[serde(flatten)]
    pub event: Event,
    pub artists: Vec<Lineup>,
}

#[derive(Debug, Deserialize)]
pub struct EventArtist {
    pub event_id: String,
    pub artist_id: String,
    pub role: Option<String>,
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

#[derive(Debug, Deserialize)]
pub struct IngestPayload {
    pub series: Vec<EventSeries>,
    pub events: Vec<IngestEvent>,
    pub days: Vec<EventDay>,
    pub artists: Vec<Artist>,
    pub event_artists: Vec<EventArtist>,
    /// Scrapers that succeeded this run: their events and line-ups here are complete.
    pub sources: Vec<String>,
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
    /// Text to find in an event's title, venue, city, organizer or line-up.
    q: Option<String>,
}

async fn get_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EventParams>,
) -> Result<Json<Vec<EventView>>, (StatusCode, String)> {
    let from_date = params.from.unwrap_or_else(|| "1970-01-01".to_string());
    let to_date = params.to.unwrap_or_else(|| "9999-12-31".to_string());
    // LIKE pattern with the user's own % and _ escaped; no query matches every title.
    let pattern = format!(
        "%{}%",
        params.q.unwrap_or_default().trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
    );

    let events = sqlx::query_as!(
        Event,
        r#"
        SELECT id, series_id, title, category, location_name, location_city, floorplan_image_url, banner_image_url, official_url, start_date, end_date, description, organizer
        FROM events
        WHERE start_date < ?1 AND end_date >= ?2
          AND (title LIKE ?3 ESCAPE '\'
            OR location_name LIKE ?3 ESCAPE '\'
            OR location_city LIKE ?3 ESCAPE '\'
            OR organizer LIKE ?3 ESCAPE '\'
            OR id IN (
                SELECT ea.event_id FROM event_artists ea
                JOIN artists a ON a.id = ea.artist_id
                WHERE a.name LIKE ?3 ESCAPE '\'
            ))
        ORDER BY start_date ASC
        "#,
        to_date,
        from_date,
        pattern
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_error)?;

    let links = sqlx::query!(
        r#"
        SELECT ea.event_id AS "event_id!", a.name, ea.role
        FROM event_artists ea
        JOIN artists a ON a.id = ea.artist_id
        JOIN events e ON e.id = ea.event_id
        WHERE e.start_date < ? AND e.end_date >= ?
        ORDER BY a.name
        "#,
        to_date,
        from_date
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_error)?;

    let mut lineups: HashMap<String, Vec<Lineup>> = HashMap::new();
    for l in links {
        lineups.entry(l.event_id).or_default().push(Lineup { name: l.name, role: l.role });
    }
    let views = events
        .into_iter()
        .map(|event| EventView { artists: lineups.remove(&event.id).unwrap_or_default(), event })
        .collect();
    Ok(Json(views))
}

fn db_error(e: sqlx::Error) -> (StatusCode, String) {
    tracing::error!("Database error: {}", e);
    (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch events".to_string())
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

    let batch_ids = serde_json::to_string(&payload.events.iter().map(|e| &e.event.id).collect::<Vec<_>>())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
    for IngestEvent { event: e, source } in payload.events {
        sqlx::query!(
            r#"
            INSERT INTO events (id, series_id, title, category, location_name, location_city, floorplan_image_url, banner_image_url, official_url, start_date, end_date, description, organizer, source)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                series_id=excluded.series_id, title=excluded.title, category=excluded.category,
                location_name=excluded.location_name, location_city=excluded.location_city,
                floorplan_image_url=excluded.floorplan_image_url, banner_image_url=excluded.banner_image_url,
                official_url=excluded.official_url, start_date=excluded.start_date, end_date=excluded.end_date,
                description=excluded.description, organizer=excluded.organizer, source=excluded.source
            "#,
            e.id, e.series_id, e.title, e.category, e.location_name, e.location_city, e.floorplan_image_url, e.banner_image_url, e.official_url, e.start_date, e.end_date, e.description, e.organizer, source
        ).execute(&mut *tx).await.map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    }

    // A source that succeeded no longer lists its events missing from this batch (dropped by the
    // site, or merged into another source's listing). Rows from before sources were tracked have none.
    for s in &payload.sources {
        sqlx::query!(
            "DELETE FROM events WHERE source = ? AND id NOT IN (SELECT value FROM json_each(?))",
            s, batch_ids
        ).execute(&mut *tx).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    sqlx::query!("DELETE FROM events WHERE source IS NULL")
        .execute(&mut *tx).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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

    // Line-ups from sources that succeeded are replaced wholesale.
    for s in &payload.sources {
        sqlx::query!("DELETE FROM event_artists WHERE source = ?", s)
            .execute(&mut *tx).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    sqlx::query!("DELETE FROM event_artists WHERE source IS NULL")
        .execute(&mut *tx).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    for l in payload.event_artists {
        sqlx::query!(
            r#"
            INSERT INTO event_artists (event_id, artist_id, role, source)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(event_id, artist_id) DO UPDATE SET role=excluded.role, source=excluded.source
            "#,
            l.event_id, l.artist_id, l.role, l.source
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

    sqlx::migrate!().run(&pool).await?;

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
