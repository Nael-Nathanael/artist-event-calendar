//! eventseye.com: trade-show directory, Indonesia listing. Pages are
//! `c1_trade-shows_indonesia.html`, then `_1.html`, `_2.html`, ... until a 404.
//! Each row has name, subtitle, city, venue and "MM/DD/YYYY" plus "N days";
//! rows dated only by month ("Oct. 2026") have no day to place and are skipped.

use async_trait::async_trait;
use chrono::{Duration, NaiveDate};
use regex::Regex;
use reqwest::{Client, StatusCode};
use scraper::{ElementRef, Html, Selector};
use uuid::Uuid;

use crate::{Event, IngestPayload, ScraperBase};

const BASE: &str = "https://www.eventseye.com/fairs/";
const MAX_PAGES: usize = 20;

fn since() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
}

pub struct EventseyeScraper;

#[async_trait]
impl ScraperBase for EventseyeScraper {
    fn name(&self) -> &'static str { "Eventseye" }

    async fn scrape(&self, client: &Client) -> Result<IngestPayload, Box<dyn std::error::Error>> {
        let mut payload = IngestPayload::default();
        for page in 0..MAX_PAGES {
            let url = match page {
                0 => format!("{BASE}c1_trade-shows_indonesia.html"),
                n => format!("{BASE}c1_trade-shows_indonesia_{n}.html"),
            };
            let res = client.get(&url).send().await?;
            if res.status() == StatusCode::NOT_FOUND {
                break;
            }
            // Pages are windows-1252 (declared only in a <meta> tag, not the Content-Type header).
            let html = res.error_for_status()?.text_with_charset("windows-1252").await?;
            let events = parse(&html, since());
            if events.is_empty() && !html.contains("class=\"tradeshows\"") {
                return Err(format!("no trade-show table on {url}").into());
            }
            payload.events.extend(events);
        }
        Ok(payload)
    }
}

fn text(e: ElementRef) -> String {
    e.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse(html: &str, since: NaiveDate) -> Vec<Event> {
    let row = Selector::parse("table.tradeshows tbody tr").unwrap();
    let td = Selector::parse("td").unwrap();
    let a = Selector::parse("a").unwrap();
    let b = Selector::parse("b").unwrap();
    let i = Selector::parse("i").unwrap();
    let date = Regex::new(r"^(\d{2})/(\d{2})/(\d{4})").unwrap();
    let days = Regex::new(r"^(\d+) days?$").unwrap();

    let mut events = Vec::new();
    for r in Html::parse_document(html).select(&row) {
        let cells: Vec<ElementRef> = r.select(&td).collect();
        let [name, _cycle, place, when] = cells[..] else { continue };

        let when_text = text(when);
        let Some(d) = date.captures(&when_text) else { continue };
        let Some(start) = NaiveDate::from_ymd_opt(d[3].parse().unwrap(), d[1].parse().unwrap(), d[2].parse().unwrap()) else {
            tracing::warn!("Eventseye: bad date {when_text:?}");
            continue;
        };
        if start < since {
            continue;
        }
        // "<td>10/07/2026<br><i>4 days</i></td>": the length sits alone in the <i>.
        let length: i64 = when
            .select(&i)
            .next()
            .and_then(|l| days.captures(&text(l)).map(|c| c[1].parse().unwrap()))
            .unwrap_or(1);
        let end = start + Duration::days(length.max(1) - 1);

        let Some(link) = name.select(&a).next() else { continue };
        let Some(href) = link.value().attr("href") else { continue };
        let title = link.select(&b).next().map(text).unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let mut place_links = place.select(&a).map(text);
        let city = place_links.next().filter(|c| !c.is_empty());
        let venue = place_links.next().filter(|v| !v.is_empty());

        events.push(Event {
            id: Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("eventseye:{href}:{start}").as_bytes()).to_string(),
            series_id: None,
            title,
            category: "TradeShow".to_string(),
            location_name: venue,
            location_city: city,
            floorplan_image_url: None,
            banner_image_url: None,
            official_url: Some(format!("{BASE}{href}")),
            start_date: format!("{start}T00:00:00+07:00"),
            end_date: format!("{end}T23:59:59+07:00"),
            description: link.select(&i).next().map(text).filter(|d| !d.is_empty()),
            organizer: None,
            source: String::new(),
        });
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from https://www.eventseye.com/fairs/c1_trade-shows_indonesia.html.
    const HTML: &str = r#"<table class="tradeshows"><caption>211 Trade Shows in Indonesia</caption>
<thead><tr><th>Exhibition Name</th><th>Cycle</th><th>Venue</th><th>Date</th></tr></thead><tbody>
<tr><td><a href="f-indo-security-9096-1.html"><b>INDO SECURITY</b><i>International Civilian Security</i></a></td><td>every 2 years</td>
  <td><a href="cy1_trade-shows-jakarta.html">Jakarta</a> <a href="pl1_trade-shows_jakarta_252.html">Jakarta International Expo (JIExpo)</a></td>
  <td>08/11/2026<br><i>3 days</i></td></tr>
<tr><td><a href="f-china-machinery-24969-1.html"><b>CHINA MACHINERY &amp; ELECTRONIC BRAND SHOW</b><i>Machinery</i></a></td><td>once a year</td>
  <td><a href="cy1_trade-shows-jakarta.html">Jakarta</a> <a href="pl1_trade-shows_jakarta_252.html">Jakarta International Expo (JIExpo)</a></td>
  <td>Oct. 2026</td></tr>
<tr><td><a href="f-allprint-indonesia-25395-1.html"><b>ALLPRINT INDONESIA</b><i>International Exhibition On Printing &amp; Packaging</i></a></td><td>once a year</td>
  <td><a href="cy1_trade-shows-jakarta.html">Jakarta</a> <a href="pl1_trade-shows_jakarta_252.html">Jakarta International Expo (JIExpo)</a></td>
  <td>10/07/2026<br><i>4 days</i></td></tr>
<tr><td><a href="f-bali-expo-1-1.html"><b>BALI EXPO</b></a></td><td>once a year</td>
  <td><a href="cy1_trade-shows-bali.html">Bali</a></td>
  <td>12/31/2026<br><i>1 day</i></td></tr>
</tbody></table>"#;

    #[test]
    fn parses_dated_rows_from_cutoff() {
        let got: Vec<_> = parse(HTML, since())
            .into_iter()
            .map(|e| (e.title, e.start_date, e.end_date, e.location_name, e.location_city, e.description, e.official_url))
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    "ALLPRINT INDONESIA".to_string(),
                    "2026-10-07T00:00:00+07:00".to_string(),
                    "2026-10-10T23:59:59+07:00".to_string(),
                    Some("Jakarta International Expo (JIExpo)".to_string()),
                    Some("Jakarta".to_string()),
                    Some("International Exhibition On Printing & Packaging".to_string()),
                    Some("https://www.eventseye.com/fairs/f-allprint-indonesia-25395-1.html".to_string()),
                ),
                (
                    "BALI EXPO".to_string(),
                    "2026-12-31T00:00:00+07:00".to_string(),
                    "2026-12-31T23:59:59+07:00".to_string(),
                    None,
                    Some("Bali".to_string()),
                    None,
                    Some("https://www.eventseye.com/fairs/f-bali-expo-1-1.html".to_string()),
                ),
            ],
            "August is before the cutoff; the month-only row has no day"
        );
    }
}
