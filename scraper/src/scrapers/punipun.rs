//! punipun.com/events: Clarissa Punipun's hand-written appearance list.
//! Month headers ("OCTOBER 2026") followed by `<br>`-separated lines like
//! "– 31 Oct-1 Nov, <strong>Title</strong>, Venue".

use async_trait::async_trait;
use chrono::NaiveDate;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use uuid::Uuid;

use crate::{Artist, Event, EventArtist, IngestPayload, ScraperBase};

const URL: &str = "https://punipun.com/events/";
const ARTIST: &str = "Clarissa Punipun";

fn since() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
}

pub struct PunipunScraper;

#[async_trait]
impl ScraperBase for PunipunScraper {
    fn name(&self) -> &'static str { "Punipun" }

    async fn scrape(&self, client: &Client) -> Result<IngestPayload, Box<dyn std::error::Error>> {
        let html = client.get(URL).send().await?.error_for_status()?.text().await?;
        Ok(parse(&html, since()))
    }
}

fn uuid(key: &str) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, key.as_bytes()).to_string()
}

fn text_of(html: &str) -> String {
    let text: String = Html::parse_fragment(html).root_element().text().collect();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn month_num(name: &str) -> Option<u32> {
    let m = name.get(..3)?.to_ascii_lowercase();
    ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
        .iter()
        .position(|&x| x == m)
        .map(|i| i as u32 + 1)
}

fn parse(html: &str, since: NaiveDate) -> IngestPayload {
    let header = Regex::new(r"^([A-Z]{3,9}) (\d{4})\b").unwrap();
    // "7 Oct," / "10-11 Oct," / "31 Oct-1 Nov," / "15 Feb."
    let line = Regex::new(
        r"^[–-] ?(\d{1,2})(?: ?([A-Za-z]{3,9}))?(?: ?- ?(\d{1,2})(?: ?([A-Za-z]{3,9}))?)? ?[,.] ?(.+)$",
    )
    .unwrap();
    let href = Selector::parse("a[href]").unwrap();

    let artist_id = uuid(&format!("artist:{ARTIST}"));
    let mut payload = IngestPayload::default();
    payload.artists.push(Artist { id: artist_id.clone(), name: ARTIST.to_string(), profile_image_url: None });

    let doc = Html::parse_document(html);
    let mut month: Option<(i32, u32)> = None;

    for p in doc.select(&Selector::parse(".entry-content p").unwrap()) {
        for chunk in p.inner_html().split("<br>") {
            let text = text_of(chunk);

            if let Some(c) = header.captures(&text) {
                if let Some(m) = month_num(&c[1]) {
                    month = Some((c[2].parse().unwrap(), m));
                    continue;
                }
            }
            let (Some((year, header_month)), Some(c)) = (month, line.captures(&text)) else { continue };

            let m1 = c.get(2).and_then(|m| month_num(m.as_str()));
            let m2 = c.get(4).and_then(|m| month_num(m.as_str()));
            let start_month = m1.or(m2).unwrap_or(header_month);
            let end_month = m2.or(m1).unwrap_or(header_month);
            let start_day: u32 = c[1].parse().unwrap();
            let end_day: u32 = c.get(3).map_or(start_day, |d| d.as_str().parse().unwrap());
            let end_year = if end_month < start_month { year + 1 } else { year };

            let (Some(start), Some(end)) = (
                NaiveDate::from_ymd_opt(year, start_month, start_day),
                NaiveDate::from_ymd_opt(end_year, end_month, end_day),
            ) else {
                tracing::warn!("Punipun: bad date in {text:?}");
                continue;
            };
            if start < since {
                continue;
            }

            // A trailing ", Venue" after the last bold part is the venue.
            let rest = c[5].trim();
            let tail = chunk.rsplit_once("</strong>").map(|(_, t)| text_of(t)).unwrap_or_default();
            let (title, venue) = match (tail.strip_prefix(',').map(str::trim), rest.strip_suffix(tail.as_str())) {
                (Some(v), Some(t)) if !v.is_empty() => (t.trim(), Some(v.to_string())),
                _ => (rest, None),
            };
            // "Guest Cosplayer at Itasha Domei" -> "Guest Cosplayer", shown if this joins Itasha Domei's line-up.
            let role = title
                .rsplit_once(" at ")
                .map(|(r, _)| r.trim_start_matches("Punipun").trim_start_matches(':').trim().to_string())
                .filter(|r| !r.is_empty());
            let title = if title.contains("Punipun") { title.to_string() } else { format!("Punipun: {title}") };

            let frag = Html::parse_fragment(chunk);
            let link = frag.select(&href).next().and_then(|a| a.value().attr("href")).unwrap_or(URL);

            let id = uuid(&format!("punipun:{start}:{title}"));
            payload.event_artists.push(EventArtist {
                event_id: id.clone(),
                artist_id: artist_id.clone(),
                role,
                source: String::new(),
            });
            payload.events.push(Event {
                id,
                series_id: None,
                title,
                category: "ArtistEvent".to_string(),
                location_name: venue,
                location_city: None,
                floorplan_image_url: None,
                banner_image_url: None,
                official_url: Some(link.to_string()),
                start_date: format!("{start}T00:00:00+07:00"),
                end_date: format!("{end}T23:59:59+07:00"),
                description: None,
                organizer: None,
                source: String::new(),
            });
        }
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"<div class="entry-content">
<p class="wp-block-paragraph"><strong>OCTOBER 2026</strong><br>&#8211; 7 Oct, <strong>Punipun Birthday!</strong><br>&#8211; 10-11 Oct, <strong><a href="https://www.instagram.com/p/DbFv7cuxmia/" target="_blank" rel="noopener">Guest Cosplayer at Itasha Domei</a></strong>, QBIG BSD</p>
<p class="wp-block-paragraph"><br>&#8211; 16 Oct, <strong>ASUS ROG Event</strong><br>&#8211; 24-25 Oct, <strong>Punipun Booth &amp; Special Performance at Indonesia Game Expo 2026</strong>, ICE BSD<br>&#8211; 31 Oct-1 Nov,<strong> [Booth AH21-22]</strong> <strong>Punipun x Koko Baju Bolong x MatchaMei Booth at COMIFURO 23 (Comic Frontier)</strong>, ICE BSD</p>
<p class="wp-block-paragraph"><strong>SEPTEMBER 2026</strong> <br>&#8211; 3 Sep, <strong>ASUS Launch Event</strong><br>&#8211; 17 Sep, <strong>Tokyo Game Show with SEGA SEA Ambassador</strong></p>
</div>"#;

    #[test]
    fn parses_events_from_cutoff() {
        let p = parse(HTML, since());
        let got: Vec<_> = p
            .events
            .iter()
            .map(|e| (e.start_date.as_str(), e.end_date.as_str(), e.title.as_str(), e.location_name.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("2026-10-07T00:00:00+07:00", "2026-10-07T23:59:59+07:00", "Punipun Birthday!", None),
                ("2026-10-10T00:00:00+07:00", "2026-10-11T23:59:59+07:00", "Punipun: Guest Cosplayer at Itasha Domei", Some("QBIG BSD")),
                ("2026-10-16T00:00:00+07:00", "2026-10-16T23:59:59+07:00", "Punipun: ASUS ROG Event", None),
                ("2026-10-24T00:00:00+07:00", "2026-10-25T23:59:59+07:00", "Punipun Booth & Special Performance at Indonesia Game Expo 2026", Some("ICE BSD")),
                ("2026-10-31T00:00:00+07:00", "2026-11-01T23:59:59+07:00", "[Booth AH21-22] Punipun x Koko Baju Bolong x MatchaMei Booth at COMIFURO 23 (Comic Frontier)", Some("ICE BSD")),
            ]
        );
        assert_eq!(p.events[1].official_url.as_deref(), Some("https://www.instagram.com/p/DbFv7cuxmia/"));
        let roles: Vec<_> = p.event_artists.iter().map(|l| l.role.as_deref()).collect();
        assert_eq!(
            roles,
            vec![
                None,
                Some("Guest Cosplayer"),
                None,
                Some("Booth & Special Performance"),
                Some("[Booth AH21-22] Punipun x Koko Baju Bolong x MatchaMei Booth"),
            ]
        );
    }
}
