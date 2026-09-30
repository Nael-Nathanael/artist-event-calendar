//! ruangcosplay.com/event: community-submitted cosplay event directory.
//! The list page links every upcoming event; each detail page carries a
//! schema.org Event JSON-LD block plus the full description and source link.

use async_trait::async_trait;
use chrono::NaiveDate;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use uuid::Uuid;

use crate::{Event, IngestPayload, ScraperBase};

const URL: &str = "https://ruangcosplay.com/event";

fn since() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
}

pub struct RuangCosplayScraper;

#[async_trait]
impl ScraperBase for RuangCosplayScraper {
    fn name(&self) -> &'static str { "RuangCosplay" }

    async fn scrape(&self, client: &Client) -> Result<IngestPayload, Box<dyn std::error::Error>> {
        let list = client.get(URL).send().await?.error_for_status()?.text().await?;
        let mut payload = IngestPayload::default();
        for url in event_urls(&list) {
            let html = match client.get(&url).send().await.and_then(|r| r.error_for_status()) {
                Ok(r) => r.text().await?,
                Err(e) => {
                    tracing::warn!("RuangCosplay: {url}: {e}");
                    continue;
                }
            };
            match parse(&url, &html, since()) {
                Ok(Some(e)) => payload.events.push(e),
                Ok(None) => {}
                Err(e) => tracing::warn!("RuangCosplay: {url}: {e}"),
            }
        }
        Ok(payload)
    }
}

/// Detail page URLs in list order, without duplicates (desktop grid and mobile list both link each event).
fn event_urls(html: &str) -> Vec<String> {
    let a = Selector::parse(r#"a[href^="https://ruangcosplay.com/event/"]"#).unwrap();
    let mut urls: Vec<String> = Vec::new();
    for href in Html::parse_document(html).select(&a).filter_map(|a| a.value().attr("href")) {
        if !urls.iter().any(|u| u == href) {
            urls.push(href.to_string());
        }
    }
    urls
}

#[derive(Deserialize)]
struct LdEvent {
    name: String,
    #[serde(rename = "startDate")]
    start_date: NaiveDate,
    #[serde(rename = "endDate")]
    end_date: Option<NaiveDate>,
    image: Option<String>,
    location: Option<LdPlace>,
    organizer: Option<LdNamed>,
}

#[derive(Deserialize)]
struct LdPlace {
    name: Option<String>,
    address: Option<LdAddress>,
}

#[derive(Deserialize)]
struct LdAddress {
    #[serde(rename = "addressLocality")]
    locality: Option<String>,
}

#[derive(Deserialize)]
struct LdNamed {
    name: Option<String>,
}

/// JSON-LD strings arrive HTML-escaped (`Q&#039;Square`, `&amp;`).
fn decode(s: &str) -> String {
    Html::parse_fragment(s).root_element().text().collect::<String>().trim().to_string()
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|s| decode(&s)).filter(|s| !s.is_empty())
}

/// HTML to plain text, one paragraph or line break per line.
fn plain_text(html: &str) -> String {
    let broken = Regex::new(r"(?i)<br\s*/?>|</p>|</li>").unwrap().replace_all(html, "\n");
    let text: String = Html::parse_fragment(&broken).root_element().text().collect();
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse(url: &str, html: &str, since: NaiveDate) -> Result<Option<Event>, Box<dyn std::error::Error>> {
    let doc = Html::parse_document(html);
    let ld = Selector::parse(r#"script[type="application/ld+json"]"#).unwrap();
    let ld = doc
        .select(&ld)
        .map(|s| s.text().collect::<String>().replace(['\n', '\r', '\t'], " "))
        .find_map(|s| serde_json::from_str::<LdEvent>(&s).ok())
        .ok_or("no Event JSON-LD")?;

    if ld.start_date < since {
        return Ok(None);
    }
    let end = ld.end_date.unwrap_or(ld.start_date);

    // The first block is the "Detail" heading and the event text; the contributor bio is nested deeper.
    let detail = Selector::parse("#div-1 > .html-format").unwrap();
    let description = doc
        .select(&detail)
        .next()
        .map(|d| plain_text(&d.inner_html()))
        .map(|t| t.strip_prefix("Detail").unwrap_or(&t).trim().to_string())
        .filter(|t| !t.is_empty());

    let link = Selector::parse("#div-1 a[target=_blank]").unwrap();
    let source = doc
        .select(&link)
        .find(|a| a.text().any(|t| t.contains("Sumber Informasi")))
        .and_then(|a| a.value().attr("href"));

    let (venue, city) = match ld.location {
        Some(l) => (non_empty(l.name), non_empty(l.address.and_then(|a| a.locality))),
        None => (None, None),
    };
    let slug = url.rsplit('/').next().unwrap_or(url);

    Ok(Some(Event {
        id: Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("ruangcosplay:{slug}").as_bytes()).to_string(),
        series_id: None,
        title: decode(&ld.name),
        category: "Cosplay".to_string(),
        location_name: venue,
        location_city: city,
        floorplan_image_url: None,
        banner_image_url: non_empty(ld.image),
        official_url: Some(source.unwrap_or(url).to_string()),
        start_date: format!("{}T00:00:00+07:00", ld.start_date),
        end_date: format!("{end}T23:59:59+07:00"),
        description,
        organizer: non_empty(ld.organizer.and_then(|o| o.name)),
        source: String::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"<div class="d-none d-md-block">
<a href="https://ruangcosplay.com/event/comic-frontier-23" class="text-decoration-none"></a>
<a href="https://ruangcosplay.com/event/chibicon-xi" class="text-decoration-none"></a></div>
<ul><li><a href="https://ruangcosplay.com/event/comic-frontier-23" class="text-decoration-none text-body fw-500"></a></li></ul>
<a href="https://ruangcosplay.com/console/profile">Edit Profil</a>"#;

    // Trimmed from https://ruangcosplay.com/event/comic-frontier-23.
    const DETAIL: &str = r#"<html><body><div id="div-1" class="col-12 mb-3">
<h1 class="h2">Comic Frontier 23</h1>
<div class="mt-4 html-format url-format">
  <p class="h4 mb-2 text-body">Detail</p>
  <p>As one journey ends, another voyage awaits us</p><p>Otsufuro #Comifuro22 &amp; let’s set our sail<br>to #Comifuro23</p>
</div>
<div class="mb-1 mt-3"><a href="https://www.instagram.com/comifuro/" class="btn btn-outline-info px-2" target="_blank">
  <svg class="icon me-1"></svg>
  Sumber Informasi
</a></div>
<div class="mt-4" id="contributor"><div class="mb-2"><div class="html-format url-format"><p>Web developer and Blogger</p></div></div></div>
</div>
<script type="application/ld+json">
{
  "@context": "https://schema.org",
  "@type": "Event",
  "name": "Comic Frontier 23 &amp; Q&#039;Square",
  "startDate": "2026-10-31",
  "endDate": "2026-11-01",
  "description": "Event cosplay di ICE BSD City...",
  "image": "https://ruangcosplay.com/images/event/2026/10/280/lg/ykNZYNEw6FTi56d3Kylu.webp",
  "location": {"@type": "Place", "name": "ICE BSD City", "address": {"@type": "PostalAddress", "addressLocality": "Kabupaten Tangerang", "addressCountry": "ID"}},
  "organizer": {"@type": "Organization", "name": "Comifuro", "url": "https://www.instagram.com/comifuro/"}
}
</script></body></html>"#;

    #[test]
    fn lists_unique_event_urls() {
        assert_eq!(
            event_urls(LIST),
            vec!["https://ruangcosplay.com/event/comic-frontier-23", "https://ruangcosplay.com/event/chibicon-xi"]
        );
    }

    #[test]
    fn parses_detail_page() {
        let e = parse("https://ruangcosplay.com/event/comic-frontier-23", DETAIL, since()).unwrap().unwrap();
        assert_eq!(e.title, "Comic Frontier 23 & Q'Square");
        assert_eq!(e.category, "Cosplay");
        assert_eq!(e.start_date, "2026-10-31T00:00:00+07:00");
        assert_eq!(e.end_date, "2026-11-01T23:59:59+07:00");
        assert_eq!(e.location_name.as_deref(), Some("ICE BSD City"));
        assert_eq!(e.location_city.as_deref(), Some("Kabupaten Tangerang"));
        assert_eq!(e.organizer.as_deref(), Some("Comifuro"));
        assert_eq!(e.official_url.as_deref(), Some("https://www.instagram.com/comifuro/"));
        assert_eq!(e.banner_image_url.as_deref(), Some("https://ruangcosplay.com/images/event/2026/10/280/lg/ykNZYNEw6FTi56d3Kylu.webp"));
        assert_eq!(
            e.description.as_deref(),
            Some("As one journey ends, another voyage awaits us\nOtsufuro #Comifuro22 & let’s set our sail\nto #Comifuro23")
        );
    }

    #[test]
    fn skips_events_before_cutoff() {
        let early = DETAIL.replace("2026-10-31", "2026-09-26");
        assert!(parse("https://ruangcosplay.com/event/x", &early, since()).unwrap().is_none());
    }
}
