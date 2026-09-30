//! exhibition.jiexpo.com/event-directory: JIExpo Kemayoran's EventON calendar.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate};
use reqwest::Client;
use uuid::Uuid;

use super::eventon::{self, Calendar};
use crate::{Event, IngestPayload, ScraperBase};

const SITE: &str = "https://exhibition.jiexpo.com";
const VENUE: &str = "JIExpo Kemayoran";

fn since() -> DateTime<FixedOffset> {
    NaiveDate::from_ymd_opt(2026, 10, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_local_timezone(eventon::wib()).unwrap()
}

pub struct JiexpoScraper;

#[async_trait]
impl ScraperBase for JiexpoScraper {
    fn name(&self) -> &'static str { "JIExpo" }

    async fn scrape(&self, client: &Client) -> Result<IngestPayload, Box<dyn std::error::Error>> {
        let cal = eventon::load(client, SITE, "/event-directory/", since()).await?;
        let classes = eventon::post_classes(client, SITE, &cal).await?;
        let types = classes
            .iter()
            .filter_map(|(id, c)| Some((*id, eventon::pascal(eventon::term(Some(c), "event_type")?))))
            .collect();
        Ok(parse(&cal, &types, since()))
    }
}

fn parse(cal: &Calendar, types: &HashMap<u64, String>, since: DateTime<FixedOffset>) -> IngestPayload {
    let rows = eventon::rows(&cal.html);
    let mut payload = IngestPayload::default();

    for e in &cal.json {
        let Some((start, end)) = eventon::span(e) else {
            tracing::warn!("JIExpo: bad timestamps for {}", e.key);
            continue;
        };
        if start < since {
            continue;
        }
        let row = rows.get(&e.key);
        let title = eventon::decode(&e.event_title);
        payload.events.push(Event {
            id: Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("jiexpo:{}", e.key).as_bytes()).to_string(),
            series_id: None,
            description: row.and_then(|r| r.description.clone()).filter(|d| !eventon::same_text(d, &title)),
            organizer: row.and_then(|r| r.organizer.clone()),
            title,
            category: types.get(&e.event_id).cloned().unwrap_or_else(|| "Exhibition".to_string()),
            location_name: Some(match row.and_then(|r| r.location.as_deref()) {
                Some(hall) => format!("{VENUE} — {hall}"),
                None => VENUE.to_string(),
            }),
            location_city: Some("Jakarta".to_string()),
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
        {"_ID":"7191_0","event_id":7191,"event_title":"ELECTRIC &#038; POWER INDONESIA 2026","unix_start":1788318000,"unix_end":1788606000},
        {"_ID":"7636_0","event_id":7636,"event_title":"SYNCHRONIZE FESTIVAL 2026","unix_start":1792116000,"unix_end":1792339200}
      ],"html":"<div id=\"event_7191_0\" class=\"eventon_list_event\"><div class=\"evo_event_schema\"><a itemprop='url' href='https://exhibition.jiexpo.com/events/electric/'></a></div><span class='evoet_data' data-d=\"{&quot;loc.n&quot;:&quot;A123 &amp;amp; D12&quot;}\"></span></div><div id=\"event_7636_0\" class=\"eventon_list_event\"><div class=\"evo_event_schema\"><a itemprop='url' href='https://exhibition.jiexpo.com/events/synchronize-festival-2026/'></a><meta itemprop='image' content=\"https://exhibition.jiexpo.com/sync.jpg\" /></div><span class='evoet_data' data-d=\"{&quot;loc.n&quot;:&quot;Gambir Expo, HALL D2 &amp;amp; OPEN SPACE&quot;,&quot;orgs&quot;:{&quot;522&quot;:&quot;PT. Pusat Kesenangan &amp;amp; Kini&quot;}}\"></span><script type=\"application/ld+json\">{\"@type\": \"Event\", \"description\":\"<p>Three days of music.<br />\nWebsite: <a href='https://synchronizefestival.com'>synchronizefestival.com</a></p>\"}</script></div>"}}}"#;

    #[test]
    fn parses_events_from_cutoff() {
        let cal = eventon::calendar(BODY).unwrap();
        let types = HashMap::from([(7636, eventon::pascal("concert-festival"))]);
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
        assert_eq!(e.organizer.as_deref(), Some("PT. Pusat Kesenangan & Kini"));
        assert_eq!(e.description.as_deref(), Some("Three days of music.\nWebsite: synchronizefestival.com"));
    }
}
