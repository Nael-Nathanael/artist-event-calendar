//! exhibition.jiexpo.com/event-directory: JIExpo Kemayoran's EventON calendar.
//! The page loads events by AJAX (`eventon_init_load`); one call with a wide focus
//! range returns every published event. Event types come from the WP REST API.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Duration, FixedOffset, NaiveDate, Utc};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use uuid::Uuid;

use crate::{Event, IngestPayload, ScraperBase};

const SITE: &str = "https://exhibition.jiexpo.com";
const VENUE: &str = "JIExpo Kemayoran";

fn wib() -> FixedOffset {
    FixedOffset::east_opt(7 * 3600).unwrap()
}

fn since() -> DateTime<FixedOffset> {
    NaiveDate::from_ymd_opt(2026, 10, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_local_timezone(wib()).unwrap()
}

pub struct JiexpoScraper;

#[async_trait]
impl ScraperBase for JiexpoScraper {
    fn name(&self) -> &'static str { "JIExpo" }

    async fn scrape(&self, client: &Client) -> Result<IngestPayload, Box<dyn std::error::Error>> {
        let page = client.get(format!("{SITE}/event-directory/")).send().await?.error_for_status()?.text().await?;
        let nonce = Regex::new(r#""n":"([0-9a-f]+)""#)?
            .captures(&page)
            .ok_or("EventON nonce not found on event directory page")?[1]
            .to_string();

        let sc = "cals[evcal_calendar_1][sc]";
        let start = since().timestamp().to_string();
        let end = (Utc::now() + Duration::days(3 * 366)).timestamp().to_string();
        let body = client
            .post(format!("{SITE}/?evo-ajax=eventon_init_load"))
            .form(&[
                (format!("{sc}[focus_start_date_range]"), start),
                (format!("{sc}[focus_end_date_range]"), end),
                ("nonce".to_string(), nonce),
            ])
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let cal = calendar(&body)?;

        let ids: Vec<String> = cal.json.iter().map(|e| e.event_id.to_string()).collect();
        let types = if ids.is_empty() { HashMap::new() } else { event_types(client, &ids).await? };
        Ok(parse(&cal, &types, since()))
    }
}

#[derive(Deserialize)]
struct Calendar {
    json: Vec<EvoEvent>,
    html: String,
}

#[derive(Deserialize)]
struct EvoEvent {
    #[serde(rename = "_ID")]
    key: String,
    event_id: u64,
    event_title: String,
    unix_start: i64,
    unix_end: i64,
}

#[derive(Deserialize)]
struct RestEvent {
    id: u64,
    class_list: Vec<String>,
}

fn calendar(body: &str) -> Result<Calendar, Box<dyn std::error::Error>> {
    #[derive(Deserialize)]
    struct Response {
        status: Option<String>,
        msg: Option<String>,
        cals: Option<serde_json::Value>,
    }
    let r: Response = serde_json::from_str(body)?;
    if r.status.as_deref() == Some("bad") {
        return Err(format!("EventON: {}", r.msg.unwrap_or_default()).into());
    }
    // `cals` is `{}`-keyed by calendar id, or `[]` when nothing matched.
    match r.cals.and_then(|c| c.as_object().and_then(|m| m.values().next().cloned())) {
        Some(c) => Ok(serde_json::from_value(c)?),
        None => Ok(Calendar { json: vec![], html: String::new() }),
    }
}

/// Event id -> PascalCase category from the `event_type-<slug>` post class.
async fn event_types(client: &Client, ids: &[String]) -> Result<HashMap<u64, String>, Box<dyn std::error::Error>> {
    let mut types = HashMap::new();
    for chunk in ids.chunks(100) {
        let url = format!("{SITE}/wp-json/wp/v2/ajde_events?include={}&per_page=100&_fields=id,class_list", chunk.join(","));
        let events: Vec<RestEvent> = client.get(url).send().await?.error_for_status()?.json().await?;
        for e in events {
            if let Some(slug) = e.class_list.iter().find_map(|c| c.strip_prefix("event_type-")) {
                types.insert(e.id, pascal(slug));
            }
        }
    }
    Ok(types)
}

fn pascal(slug: &str) -> String {
    slug.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect()
}

/// Decodes HTML entities (EventON double-encodes some, e.g. `&amp;amp;`).
fn decode(s: &str) -> String {
    let once: String = Html::parse_fragment(s).root_element().text().collect();
    if once.contains('&') && once != s { decode(&once) } else { once }
}

struct Row {
    url: Option<String>,
    image: Option<String>,
    hall: Option<String>,
}

fn rows(html: &str) -> HashMap<String, Row> {
    let row = Selector::parse(".eventon_list_event").unwrap();
    let url = Selector::parse("a[itemprop=url]").unwrap();
    let image = Selector::parse("meta[itemprop=image]").unwrap();
    let data = Selector::parse(".evoet_data").unwrap();

    Html::parse_fragment(html)
        .select(&row)
        .filter_map(|r| {
            let key = r.value().attr("id")?.strip_prefix("event_")?.to_string();
            let attr = |s: &Selector, a: &str| r.select(s).next().and_then(|e| e.value().attr(a)).map(str::to_string);
            let hall = attr(&data, "data-d")
                .and_then(|d| serde_json::from_str::<serde_json::Value>(&d).ok())
                .and_then(|d| d.get("loc.n")?.as_str().map(decode))
                .filter(|h| !h.is_empty());
            Some((key, Row { url: attr(&url, "href"), image: attr(&image, "content"), hall }))
        })
        .collect()
}

fn parse(cal: &Calendar, types: &HashMap<u64, String>, since: DateTime<FixedOffset>) -> IngestPayload {
    let rows = rows(&cal.html);
    let mut payload = IngestPayload::default();

    for e in &cal.json {
        let (Some(start), Some(end)) = (DateTime::from_timestamp(e.unix_start, 0), DateTime::from_timestamp(e.unix_end, 0)) else {
            tracing::warn!("JIExpo: bad timestamps for {}", e.key);
            continue;
        };
        let (start, end) = (start.with_timezone(&wib()), end.with_timezone(&wib()));
        if start < since {
            continue;
        }
        let row = rows.get(&e.key);
        payload.events.push(Event {
            id: Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("jiexpo:{}", e.key).as_bytes()).to_string(),
            series_id: None,
            title: decode(&e.event_title),
            category: types.get(&e.event_id).cloned().unwrap_or_else(|| "Exhibition".to_string()),
            location_name: Some(match row.and_then(|r| r.hall.as_deref()) {
                Some(hall) => format!("{VENUE} — {hall}"),
                None => VENUE.to_string(),
            }),
            location_city: Some("Jakarta".to_string()),
            floorplan_image_url: None,
            banner_image_url: row.and_then(|r| r.image.clone()),
            official_url: row.and_then(|r| r.url.clone()),
            start_date: start.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
            end_date: end.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
        });
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from a real `eventon_init_load` response.
    const BODY: &str = r#"{"cals":{"evcal_calendar_1":{"json":[
        {"_ID":"7191_0","event_id":7191,"event_title":"ELECTRIC &#038; POWER INDONESIA 2026","unix_start":1788318000,"unix_end":1788606000},
        {"_ID":"7636_0","event_id":7636,"event_title":"SYNCHRONIZE FESTIVAL 2026","unix_start":1792116000,"unix_end":1792339200}
      ],"html":"<div id=\"event_7191_0\" class=\"eventon_list_event\"><div class=\"evo_event_schema\"><a itemprop='url' href='https://exhibition.jiexpo.com/events/electric/'></a></div><span class='evoet_data' data-d=\"{&quot;loc.n&quot;:&quot;A123 &amp;amp; D12&quot;}\"></span></div><div id=\"event_7636_0\" class=\"eventon_list_event\"><div class=\"evo_event_schema\"><a itemprop='url' href='https://exhibition.jiexpo.com/events/synchronize-festival-2026/'></a><meta itemprop='image' content=\"https://exhibition.jiexpo.com/sync.jpg\" /></div><span class='evoet_data' data-d=\"{&quot;loc.n&quot;:&quot;Gambir Expo, HALL D2 &amp;amp; OPEN SPACE&quot;}\"></span></div>"}}}"#;

    #[test]
    fn parses_events_from_cutoff() {
        let cal = calendar(BODY).unwrap();
        let types = HashMap::from([(7636, pascal("concert-festival"))]);
        let p = parse(&cal, &types, since());

        assert_eq!(p.events.len(), 1, "September event is before the cutoff");
        let e = &p.events[0];
        assert_eq!(e.title, "SYNCHRONIZE FESTIVAL 2026");
        assert_eq!(e.category, "ConcertFestival");
        assert_eq!(e.start_date, "2026-10-16T09:00:00+07:00");
        assert_eq!(e.end_date, "2026-10-18T23:00:00+07:00");
        assert_eq!(e.location_name.as_deref(), Some("JIExpo Kemayoran — Gambir Expo, HALL D2 & OPEN SPACE"));
        assert_eq!(e.official_url.as_deref(), Some("https://exhibition.jiexpo.com/events/synchronize-festival-2026/"));
        assert_eq!(e.banner_image_url.as_deref(), Some("https://exhibition.jiexpo.com/sync.jpg"));
    }

    #[test]
    fn empty_calendar_and_bad_nonce() {
        assert!(calendar(r#"{"cals":[]}"#).unwrap().json.is_empty());
        assert!(calendar(r#"{"status":"bad","msg":"Nonce validation failed"}"#).is_err());
    }
}
