//! Shared client for WordPress sites running the EventON calendar plugin.
//! The calendar page loads events by AJAX (`eventon_init_load`); one call with a wide
//! focus range returns every published event. Taxonomies come from the WP REST API.

use std::collections::HashMap;

use chrono::{DateTime, Duration, FixedOffset, Utc};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;

pub fn wib() -> FixedOffset {
    FixedOffset::east_opt(7 * 3600).unwrap()
}

#[derive(Deserialize)]
pub struct Calendar {
    pub json: Vec<EvoEvent>,
    pub html: String,
}

#[derive(Deserialize)]
pub struct EvoEvent {
    #[serde(rename = "_ID")]
    pub key: String,
    pub event_id: u64,
    pub event_title: String,
    pub unix_start: i64,
    pub unix_end: i64,
}

/// Every event from `since` up to three years out, via the calendar at `{site}{page}`.
pub async fn load(client: &Client, site: &str, page: &str, since: DateTime<FixedOffset>) -> Result<Calendar, Box<dyn std::error::Error>> {
    // A page-cached copy carries a stale nonce; the query string bypasses the cache.
    let page = client
        .get(format!("{site}{page}"))
        .query(&[("nocache", Utc::now().timestamp())])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let nonce = Regex::new(r#""n":"([0-9a-f]+)""#)?
        .captures(&page)
        .ok_or("EventON nonce not found on calendar page")?[1]
        .to_string();

    let sc = "cals[evcal_calendar_1][sc]";
    let end = (Utc::now() + Duration::days(3 * 366)).timestamp().to_string();
    let body = client
        .post(format!("{site}/?evo-ajax=eventon_init_load"))
        .form(&[
            (format!("{sc}[focus_start_date_range]"), since.timestamp().to_string()),
            (format!("{sc}[focus_end_date_range]"), end),
            ("nonce".to_string(), nonce),
        ])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    calendar(&body)
}

pub fn calendar(body: &str) -> Result<Calendar, Box<dyn std::error::Error>> {
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

/// Event id -> WP post classes (`event_type-<slug>`, `event_type_2-<slug>`, ...).
pub async fn post_classes(client: &Client, site: &str, cal: &Calendar) -> Result<HashMap<u64, Vec<String>>, Box<dyn std::error::Error>> {
    #[derive(Deserialize)]
    struct RestEvent {
        id: u64,
        class_list: Vec<String>,
    }
    let ids: Vec<String> = cal.json.iter().map(|e| e.event_id.to_string()).collect();
    let mut classes = HashMap::new();
    for chunk in ids.chunks(100) {
        let url = format!("{site}/wp-json/wp/v2/ajde_events?include={}&per_page=100&_fields=id,class_list", chunk.join(","));
        let events: Vec<RestEvent> = client.get(url).send().await?.error_for_status()?.json().await?;
        classes.extend(events.into_iter().map(|e| (e.id, e.class_list)));
    }
    Ok(classes)
}

/// First term slug of `taxonomy` in a post's classes.
pub fn term<'a>(classes: Option<&'a Vec<String>>, taxonomy: &str) -> Option<&'a str> {
    let prefix = format!("{taxonomy}-");
    classes?.iter().find_map(|c| c.strip_prefix(prefix.as_str()))
}

pub fn pascal(slug: &str) -> String {
    slug.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect()
}

/// Decodes HTML entities (EventON double-encodes some, e.g. `&amp;amp;`).
pub fn decode(s: &str) -> String {
    let once: String = Html::parse_fragment(s).root_element().text().collect();
    if once.contains('&') && once != s { decode(&once) } else { once }
}

/// HTML description to plain text, one paragraph or line break per line.
fn plain_text(html: &str) -> String {
    let broken = Regex::new(r"(?i)<br\s*/?>|</p>|</li>|</h\d>").unwrap().replace_all(html, "\n");
    decode(&broken)
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// EventON falls back to the title (sometimes quoted, dashes swapped) when an event has no description.
pub fn same_text(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    norm(a) == norm(b)
}

pub struct Row {
    pub url: Option<String>,
    pub image: Option<String>,
    pub location: Option<String>,
    pub organizer: Option<String>,
    pub description: Option<String>,
}

/// Per-event details from the calendar HTML, keyed by `EvoEvent::key`.
pub fn rows(html: &str) -> HashMap<String, Row> {
    let row = Selector::parse(".eventon_list_event").unwrap();
    let url = Selector::parse("a[itemprop=url]").unwrap();
    let image = Selector::parse("meta[itemprop=image]").unwrap();
    let data = Selector::parse(".evoet_data").unwrap();
    let ld = Selector::parse(r#"script[type="application/ld+json"]"#).unwrap();

    Html::parse_fragment(html)
        .select(&row)
        .filter_map(|r| {
            let key = r.value().attr("id")?.strip_prefix("event_")?.to_string();
            let attr = |s: &Selector, a: &str| r.select(s).next().and_then(|e| e.value().attr(a)).map(str::to_string);
            let data = attr(&data, "data-d").and_then(|d| serde_json::from_str::<serde_json::Value>(&d).ok());
            let location = data
                .as_ref()
                .and_then(|d| d.get("loc.n")?.as_str().map(decode))
                .filter(|h| !h.is_empty());
            let organizer = data
                .as_ref()
                .and_then(|d| d.get("orgs")?.as_object().map(|o| o.values().filter_map(|v| v.as_str()).map(decode).collect::<Vec<_>>().join(", ")))
                .filter(|o| !o.is_empty());
            // EventON leaves raw newlines inside JSON-LD strings, which strict JSON rejects.
            let description = r
                .select(&ld)
                .next()
                .map(|s| s.text().collect::<String>().replace(['\n', '\r', '\t'], " "))
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|j| j.get("description")?.as_str().map(plain_text))
                .filter(|d| !d.is_empty());
            Some((key, Row { url: attr(&url, "href"), image: attr(&image, "content"), location, organizer, description }))
        })
        .collect()
}

/// Start and end in WIB; an end before the start (a data-entry slip) collapses to the start.
pub fn span(e: &EvoEvent) -> Option<(DateTime<FixedOffset>, DateTime<FixedOffset>)> {
    let start = DateTime::from_timestamp(e.unix_start, 0)?.with_timezone(&wib());
    let end = DateTime::from_timestamp(e.unix_end, 0)?.with_timezone(&wib());
    Some((start, end.max(start)))
}

pub fn iso(t: DateTime<FixedOffset>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_fallback_is_not_a_description() {
        assert!(same_text("'ART JAKARTA 2026'", "ART JAKARTA 2026"));
        assert!(same_text("'PAP EXPO - ASIAN PULP'", "PAP EXPO – ASIAN PULP"));
        assert!(!same_text("http://andreabocelli-indonesia.com", "Andrea Bocelli: Romanza World Tour"));
    }

    #[test]
    fn empty_calendar_and_bad_nonce() {
        assert!(calendar(r#"{"cals":[]}"#).unwrap().json.is_empty());
        assert!(calendar(r#"{"status":"bad","msg":"Nonce validation failed"}"#).is_err());
    }

    #[test]
    fn terms_and_spans() {
        let classes = vec!["event_type-musik-konser".to_string(), "event_type_2-jawa-barat".to_string()];
        assert_eq!(term(Some(&classes), "event_type"), Some("musik-konser"));
        assert_eq!(term(Some(&classes), "event_type_2"), Some("jawa-barat"));
        assert_eq!(pascal("concert-festival"), "ConcertFestival");

        let e = EvoEvent { key: "1_0".into(), event_id: 1, event_title: String::new(), unix_start: 1790866800, unix_end: 1790863200 };
        let (start, end) = span(&e).unwrap();
        assert_eq!((iso(start), iso(end)), ("2026-10-01T22:00:00+07:00".to_string(), "2026-10-01T22:00:00+07:00".to_string()));
    }
}
