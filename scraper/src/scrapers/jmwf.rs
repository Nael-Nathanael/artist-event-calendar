//! instagram.com/jmwf.id: Jakarta Mega Wedding Festival. Instagram asks plain HTTP
//! clients to log in, so the profile loads in a headless browser (FlareSolverr at
//! `FLARESOLVERR_URL`). Recent post captions sit in the page's JSON and announce each
//! edition with "📅 2–4 Oktober 2026" and "📍 JIEXPO Kemayoran, Hall D2" lines.

use std::collections::BTreeMap;
use std::env;
use std::time::Duration;

use async_trait::async_trait;
use chrono::NaiveDate;
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use uuid::Uuid;

use crate::{Event, IngestPayload, ScraperBase};

const PROFILE: &str = "https://www.instagram.com/jmwf.id/";
const TITLE: &str = "Jakarta Mega Wedding Festival";

fn since() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
}

pub struct JmwfScraper;

#[async_trait]
impl ScraperBase for JmwfScraper {
    fn name(&self) -> &'static str { "JMWF" }

    /// Editions are announced months ahead; a monthly look is enough.
    fn interval(&self) -> Duration {
        Duration::from_secs(30 * 24 * 3600)
    }

    async fn scrape(&self, client: &Client) -> Result<IngestPayload, Box<dyn std::error::Error>> {
        #[derive(Deserialize)]
        struct Reply {
            status: String,
            message: String,
            solution: Option<Solution>,
        }
        #[derive(Deserialize)]
        struct Solution {
            status: u16,
            response: String,
        }

        let solver = env::var("FLARESOLVERR_URL").map_err(|_| "FLARESOLVERR_URL is not set")?;
        let reply: Reply = client
            .post(solver)
            .json(&serde_json::json!({ "cmd": "request.get", "url": PROFILE, "maxTimeout": 90000, "waitInSeconds": 5 }))
            .timeout(Duration::from_secs(150))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let page = match reply.solution {
            Some(s) if reply.status == "ok" && s.status == 200 => s.response,
            Some(s) => return Err(format!("FlareSolverr: HTTP {} for {PROFILE}", s.status).into()),
            None => return Err(format!("FlareSolverr: {}", reply.message).into()),
        };
        if !page.contains(r#""caption":{"text":"#) {
            return Err("no post captions on the profile page (login wall?)".into());
        }
        Ok(IngestPayload { events: parse(&page, since()), ..Default::default() })
    }
}

fn month(name: &str) -> Option<u32> {
    let m = name.get(..3)?.to_lowercase();
    let m = match m.as_str() {
        "mei" => "may",
        "agu" => "aug",
        "okt" => "oct",
        "des" => "dec",
        m => m,
    };
    ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
        .iter()
        .position(|&x| x == m)
        .map(|i| i as u32 + 1)
}

/// "2–4 Oktober 2026", "30 Okt – 1 Nov 2026" or "5 October 2026".
fn date_range(line: &str) -> Option<(NaiveDate, NaiveDate)> {
    let re = Regex::new(r"(\d{1,2})(?:\s+([A-Za-z]+))?\s*(?:[–—-]\s*(\d{1,2}))?\s+([A-Za-z]+)\s+(\d{4})").unwrap();
    let c = re.captures(line)?;
    let year: i32 = c[5].parse().ok()?;
    let end_month = month(&c[4])?;
    let start_month = c.get(2).and_then(|m| month(m.as_str())).unwrap_or(end_month);
    let start_day: u32 = c[1].parse().ok()?;
    let end_day: u32 = c.get(3).map_or(Some(start_day), |d| d.as_str().parse().ok())?;
    let start_year = if start_month > end_month { year - 1 } else { year };
    Some((NaiveDate::from_ymd_opt(start_year, start_month, start_day)?, NaiveDate::from_ymd_opt(year, end_month, end_day)?))
}

fn parse(page: &str, since: NaiveDate) -> Vec<Event> {
    let caption = Regex::new(r#""caption":\{"text":"((?:[^"\\]|\\.)*)""#).unwrap();
    // Newest post first, so the first caption naming an edition supplies its venue and text.
    let mut editions: BTreeMap<(NaiveDate, NaiveDate), (Option<String>, String)> = BTreeMap::new();
    for c in caption.captures_iter(page) {
        let Ok(text) = serde_json::from_str::<String>(&format!("\"{}\"", &c[1])) else { continue };
        let Some(range) = text.lines().filter(|l| l.contains('📅')).find_map(date_range) else { continue };
        let venue = text
            .lines()
            .find_map(|l| l.split_once('📍'))
            .map(|(_, v)| v.trim().to_string())
            .filter(|v| !v.is_empty());
        editions.entry(range).or_insert((venue, text));
    }

    editions
        .into_iter()
        .filter(|((start, _), _)| *start >= since)
        .map(|((start, end), (venue, text))| Event {
            id: Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("jmwf:{start}").as_bytes()).to_string(),
            series_id: None,
            title: TITLE.to_string(),
            category: "Exhibition".to_string(),
            location_name: venue,
            location_city: Some("Jakarta".to_string()),
            floorplan_image_url: None,
            banner_image_url: None,
            official_url: Some(PROFILE.to_string()),
            start_date: format!("{start}T00:00:00+07:00"),
            end_date: format!("{end}T23:59:59+07:00"),
            description: Some(text),
            organizer: Some("Cantik Wedding Company".to_string()),
            source: String::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Caption JSON as embedded in the rendered profile page.
    const PAGE: &str = r#"{"caption":{"text":"💍 NEXT WEEK, WE’RE GETTING MARRIED! 💍\n\n📅 2–4 October 2026\n📍 JIEXPO Kemayoran, Hall D2\n🎟️ FREE ENTRY"},"seo_canonical_url":null}
{"caption":{"text":"Khusus pemegang Kartu Kredit Mandiri\n\n📅 2–4 Oktober 2026\n📍 JIEXPO Kemayoran, Hall D\n\nBatch 1: 15–16 September 2026"},"seo_canonical_url":null}
{"caption":{"text":"The 90th edition\n📅 1–3 Mei 2026\n📍 JIEXPO Kemayoran, Hall D"}}
{"caption":{"text":"Year-end special\n📅 30 Des – 1 Jan 2027\n📍 JCC Senayan"}}
{"caption":{"text":"Thank you for coming! No date line here."}}"#;

    #[test]
    fn one_event_per_announced_edition() {
        let got: Vec<_> = parse(PAGE, since())
            .into_iter()
            .map(|e| (e.start_date, e.end_date, e.location_name))
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    "2026-10-02T00:00:00+07:00".to_string(),
                    "2026-10-04T23:59:59+07:00".to_string(),
                    Some("JIEXPO Kemayoran, Hall D2".to_string()),
                ),
                (
                    "2026-12-30T00:00:00+07:00".to_string(),
                    "2027-01-01T23:59:59+07:00".to_string(),
                    Some("JCC Senayan".to_string()),
                ),
            ],
            "May is before the cutoff; the promo batch line has no 📅"
        );
    }

    #[test]
    fn reads_indonesian_and_english_months() {
        let d = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        assert_eq!(date_range("📅 2–4 Oktober 2026"), Some((d(2026, 10, 2), d(2026, 10, 4))));
        assert_eq!(date_range("📅 5 August 2027"), Some((d(2027, 8, 5), d(2027, 8, 5))));
        assert_eq!(date_range("📅 30 Okt – 1 Nov 2026"), Some((d(2026, 10, 30), d(2026, 11, 1))));
        assert_eq!(date_range("📅 TBA"), None);
    }
}
