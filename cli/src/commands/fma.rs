//! `flint fma` — Fleet Maintained Apps: slug search, app details, recent
//! updates, and the feed-driven registry refresh.
//!
//! The engine (`flint_lint::fma`) owns the registry and stays network-free;
//! this module owns the one network action — fetching the fmalibrary.com RSS
//! feed — and writes the cache overlay the engine, LSP, and lint rule read.

use crate::args::FmaKind;
use anyhow::{Context, Result};
use colored::Colorize;
use flint_lint::fma::{self, FmaApp};

/// Default feed; override with FLINT_FMA_FEED_URL (mirrors, testing).
const FEED_URL: &str = "https://fmalibrary.com/feed.xml";

/// Cache older than this earns a staleness hint on search/show/latest.
const STALE_AFTER_DAYS: u64 = 30;

pub(crate) fn run(what: FmaKind) -> Result<()> {
    match what {
        FmaKind::Search { term, json } => search(&term, json),
        FmaKind::Show { slug, json } => show(&slug, json),
        FmaKind::Latest { days, json } => latest(days, json),
        FmaKind::Refresh => refresh(),
    }
}

fn search(term: &str, json: bool) -> Result<()> {
    let hits = fma::search(term);
    if json {
        println!("{}", serde_json::to_string_pretty(&apps_json(&hits))?);
        return Ok(());
    }
    if hits.is_empty() {
        println!(
            "no app matching '{term}' — try `flint fma refresh` (registry may lag Fleet), or check fmalibrary.com"
        );
        return Ok(());
    }
    for app in &hits {
        for platform in &app.platforms {
            let slug = format!("{}/{}", app.name, platform);
            match &app.latest_version {
                Some(v) => println!("{slug}  {}", format!("(latest {v})").dimmed()),
                None => println!("{slug}"),
            }
        }
    }
    staleness_hint();
    Ok(())
}

fn show(query: &str, json: bool) -> Result<()> {
    // Accept both "name" and "name/platform".
    let name = query.split('/').next().unwrap_or(query);
    let Some(app) = fma::APPS.iter().find(|a| a.name == name) else {
        let hint = fma::find_similar_slug(query)
            .map(|s| format!(" Did you mean '{s}'?"))
            .unwrap_or_default();
        anyhow::bail!("unknown app '{query}'.{hint}");
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&app_json(app))?);
        return Ok(());
    }
    println!("{}", app.name.bold());
    for platform in &app.platforms {
        println!("  slug: {}/{}", app.name, platform);
    }
    if let Some(v) = &app.latest_version {
        println!("  latest: {v}");
    }
    if let Some(u) = &app.installer_url {
        println!("  installer: {u}");
    }
    if let Some(d) = &app.updated {
        println!("  updated: {d}");
    }
    if app.latest_version.is_none() {
        println!(
            "  {}",
            "version/installer unknown — run `flint fma refresh` to pull feed data".dimmed()
        );
    }
    staleness_hint();
    Ok(())
}

fn latest(days: u32, json: bool) -> Result<()> {
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(u64::from(days) * 86_400);
    let recent: Vec<&FmaApp> = fma::APPS
        .iter()
        .filter(|a| {
            a.updated
                .as_deref()
                .and_then(parse_rfc2822_ish)
                .is_some_and(|t| t >= cutoff)
        })
        .collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&apps_json(&recent))?);
        return Ok(());
    }
    if recent.is_empty() {
        println!("no updates in the cache from the last {days} day(s) — run `flint fma refresh` first");
        return Ok(());
    }
    for app in &recent {
        println!(
            "{}  {}  {}",
            app.name.bold(),
            app.latest_version.as_deref().unwrap_or("?"),
            app.updated.as_deref().unwrap_or("").dimmed()
        );
    }
    Ok(())
}

fn refresh() -> Result<()> {
    let url = std::env::var("FLINT_FMA_FEED_URL").unwrap_or_else(|_| FEED_URL.to_string());
    println!("fetching {url}…");
    let body = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .with_context(|| format!("failed to fetch {url}"))?
        .into_string()
        .context("feed body is not valid UTF-8")?;

    let updates = parse_feed(&body)?;
    if updates.is_empty() {
        anyhow::bail!("feed parsed but contained no app updates — not overwriting the cache");
    }

    let path = fma::cache_path().context("cannot resolve cache dir (no HOME/XDG_CACHE_HOME)")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, render_cache_toml(&updates))?;
    println!(
        "{} {} app update(s) → {}",
        "✓".green(),
        updates.len(),
        path.display()
    );
    Ok(())
}

/// One update parsed from a feed item.
struct FeedUpdate {
    name: String,
    slug_name: String,
    platform: String,
    new_version: String,
    installer_url: Option<String>,
    pub_date: Option<String>,
}

/// Parse the fmalibrary RSS. Per item:
///   <guid>  = "<slug>/<platform>-<old>-<new>"   (slug + platform + versions)
///   <title> = "Name old → new (Mac)"            (human name)
///   <description> contains  <a href="URL">Download installer</a>
fn parse_feed(xml: &str) -> Result<Vec<FeedUpdate>> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut updates: Vec<FeedUpdate> = Vec::new();

    let (mut in_item, mut field) = (false, None::<String>);
    let (mut title, mut guid, mut desc, mut date) = (String::new(), String::new(), String::new(), String::new());

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == "item" {
                    in_item = true;
                    title.clear();
                    guid.clear();
                    desc.clear();
                    date.clear();
                } else if in_item {
                    field = Some(tag);
                }
            }
            Ok(Event::Text(t)) if in_item => {
                let text = t.unescape().unwrap_or_default().to_string();
                match field.as_deref() {
                    Some("title") => title.push_str(&text),
                    Some("guid") => guid.push_str(&text),
                    Some("description") => desc.push_str(&text),
                    Some("pubDate") => date.push_str(&text),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == "item" {
                    in_item = false;
                    if let Some(u) = update_from_parts(&title, &guid, &desc, &date) {
                        updates.push(u);
                    }
                } else {
                    field = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => anyhow::bail!("feed XML parse error: {e}"),
            _ => {}
        }
        buf.clear();
    }
    Ok(updates)
}

/// guid "slack/darwin-1.2.3-1.2.4" → (slack, darwin, 1.2.4); title supplies
/// the human name ("Slack 1.2.3 → 1.2.4 (Mac)" → "Slack").
fn update_from_parts(title: &str, guid: &str, desc: &str, date: &str) -> Option<FeedUpdate> {
    let (slug_name, rest) = guid.split_once('/')?;
    // rest = "<platform>-<old>-<new>"; versions may contain dashes only in
    // theory — split from the LEFT for platform, from the RIGHT is unreliable
    // (versions contain dots, not dashes, in practice).
    let mut parts = rest.splitn(2, '-');
    let platform = parts.next()?.to_string();
    let versions = parts.next()?;
    let new_version = versions.rsplit('-').next()?.to_string();

    let name = title
        .split(|c: char| c.is_ascii_digit())
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(slug_name)
        .to_string();

    let installer_url = desc
        .split("href=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .map(str::to_string);

    Some(FeedUpdate {
        name,
        slug_name: slug_name.to_string(),
        platform,
        new_version,
        installer_url,
        pub_date: (!date.is_empty()).then(|| date.to_string()),
    })
}

/// Render the cache overlay in the registry schema (grouped by app; a feed
/// carries one platform per item, the engine unions platforms on merge).
fn render_cache_toml(updates: &[FeedUpdate]) -> String {
    use std::collections::BTreeMap;
    // Keep only the newest entry per (app, platform): the feed lists newest
    // first, so first-wins.
    let mut seen: BTreeMap<(String, String), &FeedUpdate> = BTreeMap::new();
    for u in updates {
        seen.entry((u.slug_name.clone(), u.platform.clone())).or_insert(u);
    }

    let mut out = String::from(
        "# Auto-generated by `flint fma refresh` from fmalibrary.com/feed.xml.\n\
         # Overlays the bundled registry (platforms union; version/url/date win).\n\n",
    );
    for ((slug_name, platform), u) in &seen {
        out.push_str("[[fma]]\n");
        out.push_str(&format!("name = {:?}\n", slug_name));
        out.push_str(&format!("platforms = [{:?}]\n", platform));
        out.push_str(&format!("latest_version = {:?}\n", u.new_version));
        if let Some(url) = &u.installer_url {
            out.push_str(&format!("installer_url = {:?}\n", url));
        }
        if let Some(d) = &u.pub_date {
            out.push_str(&format!("updated = {:?}\n", d));
        }
        let _ = &u.name; // human name intentionally not cached — registry keys on slug names
        out.push('\n');
    }
    out
}

/// Parse RFC-2822-ish feed dates ("Thu, 02 Jul 2026 21:10:50 +0000") into
/// SystemTime — day-granularity is all `latest --days` needs, so a tiny
/// hand parser beats a chrono dependency here.
fn parse_rfc2822_ish(s: &str) -> Option<std::time::SystemTime> {
    let mut it = s.split_whitespace();
    let _weekday = it.next()?;
    let day: u64 = it.next()?.parse().ok()?;
    let month = match it.next()? {
        "Jan" => 1, "Feb" => 2, "Mar" => 3, "Apr" => 4, "May" => 5, "Jun" => 6,
        "Jul" => 7, "Aug" => 8, "Sep" => 9, "Oct" => 10, "Nov" => 11, "Dec" => 12,
        _ => return None,
    };
    let year: u64 = it.next()?.parse().ok()?;

    // Days since Unix epoch (civil-from-days inverse, Howard Hinnant's algo).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(days * 86_400))
}

fn staleness_hint() {
    let Some(path) = fma::cache_path() else { return };
    let stale = match std::fs::metadata(&path).and_then(|m| m.modified()) {
        Ok(modified) => modified
            .elapsed()
            .map(|e| e.as_secs() > STALE_AFTER_DAYS * 86_400)
            .unwrap_or(false),
        Err(_) => true, // no cache at all
    };
    if stale {
        eprintln!(
            "{}",
            "hint: registry data may be stale — run `flint fma refresh`".dimmed()
        );
    }
}

fn app_json(app: &FmaApp) -> serde_json::Value {
    serde_json::json!({
        "name": app.name,
        "slugs": app.platforms.iter().map(|p| format!("{}/{}", app.name, p)).collect::<Vec<_>>(),
        "latest_version": app.latest_version,
        "installer_url": app.installer_url,
        "updated": app.updated,
    })
}

fn apps_json(apps: &[&FmaApp]) -> serde_json::Value {
    serde_json::json!({
        "apps": apps.iter().map(|a| app_json(a)).collect::<Vec<_>>(),
        "total": apps.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
<item>
  <title>Raycast 1.104.20 → 1.104.21 (Mac)</title>
  <link>https://fmalibrary.com</link>
  <description>Raycast has been updated. &lt;a href=&quot;https://releases.raycast.com/releases/1.104.21/download?build=arm&quot;&gt;Download installer&lt;/a&gt;</description>
  <pubDate>Thu, 02 Jul 2026 21:10:50 +0000</pubDate>
  <guid isPermaLink="false">raycast/darwin-1.104.20-1.104.21</guid>
</item>
<item>
  <title>7-Zip 24.08 → 24.09 (Windows)</title>
  <description>no link here</description>
  <pubDate>Wed, 01 Jul 2026 10:00:00 +0000</pubDate>
  <guid isPermaLink="false">7-zip/windows-24.08-24.09</guid>
</item>
</channel></rss>"#;

    #[test]
    fn parses_feed_items() {
        let updates = parse_feed(SAMPLE).unwrap();
        assert_eq!(updates.len(), 2);
        let r = &updates[0];
        assert_eq!(r.slug_name, "raycast");
        assert_eq!(r.platform, "darwin");
        assert_eq!(r.new_version, "1.104.21");
        assert_eq!(
            r.installer_url.as_deref(),
            Some("https://releases.raycast.com/releases/1.104.21/download?build=arm")
        );
        // App names with digits/dashes keep their slug identity.
        assert_eq!(updates[1].slug_name, "7-zip");
        assert_eq!(updates[1].platform, "windows");
        assert_eq!(updates[1].new_version, "24.09");
        assert!(updates[1].installer_url.is_none());
    }

    #[test]
    fn cache_toml_round_trips_via_engine_schema() {
        let updates = parse_feed(SAMPLE).unwrap();
        let toml_src = render_cache_toml(&updates);
        // Must parse under the same schema the engine overlay reads.
        #[derive(serde::Deserialize)]
        struct F { fma: Vec<flint_lint::fma::FmaApp> }
        let parsed: F = toml::from_str(&toml_src).expect("cache TOML parses");
        assert_eq!(parsed.fma.len(), 2);
        let ray = parsed.fma.iter().find(|a| a.name == "raycast").unwrap();
        assert_eq!(ray.latest_version.as_deref(), Some("1.104.21"));
        assert_eq!(ray.platforms, vec!["darwin"]);
    }

    #[test]
    fn feed_dates_parse_to_day_granularity() {
        let t = parse_rfc2822_ish("Thu, 02 Jul 2026 21:10:50 +0000").unwrap();
        let days = t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 86_400;
        // 2026-07-02 = 20 636 days after 1970-01-01.
        assert_eq!(days, 20_636);
    }
}
