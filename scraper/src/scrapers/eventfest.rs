//! eventfest.id/kalender-event: Indonesian event listing on EventON.
//! Category is the `event_type` term, city the `event_type_2` term.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate};
use reqwest::Client;
use serde::Deserialize;
use uuid::Uuid;

use super::eventon::{self, Calendar};
use crate::{Event, IngestPayload, ScraperBase};

const SITE: &str = "https://eventfest.id";

fn since() -> DateTime<FixedOffset> {
    NaiveDate::from_ymd_opt(2026, 10, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_local_timezone(eventon::wib()).unwrap()
}

pub struct EventfestScraper;

#[async_trait]
impl ScraperBase for EventfestScraper {
    fn name(&self) -> &'static str { "Eventfest" }

    async fn scrape(&self, client: &Client) -> Result<IngestPayload, Box<dyn std::error::Error>> {
        let cal = eventon::load(client, SITE, "/kalender-event/", since()).await?;
        let classes = eventon::post_classes(client, SITE, &cal).await?;
        let cities = city_names(client).await?;
        Ok(parse(&cal, &classes, &cities, since()))
    }
}

/// `event_type_2` slug -> display name ("jawa-barat" -> "Jawa Barat").
async fn city_names(client: &Client) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    #[derive(Deserialize)]
    struct Term {
        slug: String,
        name: String,
    }
    let terms: Vec<Term> = client
        .get(format!("{SITE}/wp-json/wp/v2/event_type_2?per_page=100&_fields=slug,name"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(terms.into_iter().map(|t| (t.slug, eventon::decode(&t.name))).collect())
}

fn parse(
    cal: &Calendar,
    classes: &HashMap<u64, Vec<String>>,
    cities: &HashMap<String, String>,
    since: DateTime<FixedOffset>,
) -> IngestPayload {
    let rows = eventon::rows(&cal.html);
    let mut payload = IngestPayload::default();

    for e in &cal.json {
        let Some((start, end)) = eventon::span(e) else {
            tracing::warn!("Eventfest: bad timestamps for {}", e.key);
            continue;
        };
        if start < since {
            continue;
        }
        let row = rows.get(&e.key);
        let c = classes.get(&e.event_id);
        let title = eventon::decode(&e.event_title);
        payload.events.push(Event {
            id: Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("eventfest:{}", e.key).as_bytes()).to_string(),
            series_id: None,
            description: row.and_then(|r| r.description.clone()).filter(|d| !eventon::same_text(d, &title)),
            organizer: row.and_then(|r| r.organizer.clone()),
            title,
            category: eventon::term(c, "event_type").map_or_else(|| "Event".to_string(), eventon::pascal),
            location_name: row.and_then(|r| r.location.clone()),
            location_city: eventon::term(c, "event_type_2").and_then(|s| cities.get(s).cloned()),
            floorplan_image_url: None,
            banner_image_url: row.and_then(|r| r.image.clone()),
            official_url: row.and_then(|r| r.url.clone()),
            start_date: eventon::iso(start),
            end_date: eventon::iso(end),
        });
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from a real `eventon_init_load` response.
    const BODY: &str = r#"{"cals":{"evcal_calendar_1":{"json":[
        {"_ID":"11449_0","event_id":11449,"event_title":"JICAF 2026","unix_start":1789578000,"unix_end":1791133199},
        {"_ID":"6000_0","event_id":6000,"event_title":"Indonesia Comic Con 2026","unix_start":1790960400,"unix_end":1791133199}
      ],"html":"<div id=\"event_6000_0\" class=\"eventon_list_event\"><div class=\"evo_event_schema\"><a itemprop='url'  href='https://eventfest.id/events/indonesia-comic-con-2026/'></a><meta itemprop='image' content=\"https://eventfest.id/wp-content/uploads/2026/02/1178630484.jpg\" /></div><span class='evoet_data' data-d=\"{&quot;loc.n&quot;:&quot;NICE PIK 2, Tangerang&quot;,&quot;orgs&quot;:[]}\"></span><script type=\"application/ld+json\">{\"@type\": \"Event\", \"description\":\"<p dir='ltr'>Satu dekade merayakan kreativitas!</p> <ul> <li dir='ltr'><b>Tema:</b> A Decade of Pop Culture Fiesta</li> <li dir='ltr'><b>Penyelenggara:</b> Panorama Media</li> </ul> \"}</script></div>"}}}"#;

    #[test]
    fn parses_events_from_cutoff() {
        let cal = eventon::calendar(BODY).unwrap();
        let classes = HashMap::from([(
            6000,
            vec!["ajde_events".to_string(), "event_type-festival-hiburan".to_string(), "event_type_2-tangerang".to_string()],
        )]);
        let cities = HashMap::from([("tangerang".to_string(), "Tangerang".to_string())]);
        let p = parse(&cal, &classes, &cities, since());

        assert_eq!(p.events.len(), 1, "JICAF starts in September, before the cutoff");
        let e = &p.events[0];
        assert_eq!(e.title, "Indonesia Comic Con 2026");
        assert_eq!(e.category, "FestivalHiburan");
        assert_eq!(e.start_date, "2026-10-03T00:00:00+07:00");
        assert_eq!(e.end_date, "2026-10-04T23:59:59+07:00");
        assert_eq!(e.location_name.as_deref(), Some("NICE PIK 2, Tangerang"));
        assert_eq!(e.location_city.as_deref(), Some("Tangerang"));
        assert_eq!(e.official_url.as_deref(), Some("https://eventfest.id/events/indonesia-comic-con-2026/"));
        assert_eq!(e.banner_image_url.as_deref(), Some("https://eventfest.id/wp-content/uploads/2026/02/1178630484.jpg"));
        assert_eq!(e.organizer, None);
        assert_eq!(
            e.description.as_deref(),
            Some("Satu dekade merayakan kreativitas!\nTema: A Decade of Pop Culture Fiesta\nPenyelenggara: Panorama Media")
        );
    }
}
