//! Metadata and art from VNDB (https://vndb.org), via its public API.

use anyhow::{Context, Result};
use serde_json::json;

const API: &str = "https://api.vndb.org/kana/vn";
const FIELDS: &str = "id,title,alttitle,aliases,titles.title,titles.lang,titles.official,\
    released,votecount,developers.name,\
    image.url,image.sexual,image.violence,\
    screenshots.url,screenshots.sexual,screenshots.violence,screenshots.dims";

/// VNDB rates images 0 (safe) to 2 (explicit). Library art stays below this.
const MAX_SEXUAL: f64 = 1.0;
const MAX_VIOLENCE: f64 = 1.0;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Meta {
    pub id: String,
    pub title: String,
    /// The official English title, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub english_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub developers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_url: Option<String>,
}

fn query(filters: serde_json::Value, results: u32) -> Result<Vec<serde_json::Value>> {
    let mut body = json!({ "filters": filters, "fields": FIELDS, "results": results });
    // `searchrank` is only valid together with a search filter.
    if body["filters"][0] == "search" {
        body["sort"] = json!("searchrank");
    }
    let text = crate::download::post_json(API, &body).context("querying VNDB")?;
    let v: serde_json::Value = serde_json::from_str(&text)?;
    Ok(v["results"].as_array().cloned().unwrap_or_default())
}

/// Best guess for a title. Search rank alone is unreliable (an obscure 1996
/// "DIES IRAE" outranks the famous one), so take the most-voted of the top hits.
pub fn search(title: &str) -> Result<Option<Meta>> {
    let hits = query(json!(["search", "=", title]), 8)?;
    Ok(hits
        .iter()
        .filter(|h| names_match(title, h))
        .max_by_key(|h| h["votecount"].as_u64().unwrap_or(0))
        .map(parse))
}

/// Words that say which release, not which game.
const NOISE: &[&str] = &[
    "r18",
    "18",
    "unrated",
    "uncensored",
    "hd",
    "complete",
    "edition",
    "release",
    "version",
    "setup",
    "installer",
];

/// Lowercase words, also split at case and letter/digit changes
/// (`RiddleJokerR18` -> riddle, joker, r18), noise dropped.
fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev = None;
            continue;
        }
        if let Some(p) = prev {
            let boundary = (p.is_lowercase() && c.is_uppercase())
                || (p.is_ascii_digit() && c.is_ascii_uppercase());
            if boundary && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        }
        cur.extend(c.to_lowercase());
        prev = Some(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.retain(|w| !NOISE.contains(&w.as_str()));
    out
}

/// Whether a search hit is really the game we asked about. VNDB search is
/// fuzzy: "engine" finds "My Protogen Engineer". Accept a hit only if one
/// of its names (any language, or an alias like "GSS") contains every
/// word we searched for, or matches it ignoring spaces ("RiddleJoker").
fn names_match(query: &str, hit: &serde_json::Value) -> bool {
    let want = words(query);
    if want.is_empty() {
        return false;
    }
    let compact_want: String = want.concat();
    let mut names: Vec<String> = ["title", "alttitle"]
        .iter()
        .filter_map(|k| hit[*k].as_str().map(String::from))
        .collect();
    names.extend(
        hit["aliases"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a.as_str().map(String::from)),
    );
    names.extend(
        hit["titles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t["title"].as_str().map(String::from)),
    );
    names.iter().any(|n| {
        let have = words(n);
        want.iter().all(|w| have.contains(w)) || have.concat() == compact_want
    })
}

pub fn by_id(id: &str) -> Result<Option<Meta>> {
    Ok(query(json!(["id", "=", id]), 1)?.first().map(parse))
}

fn safe(img: &serde_json::Value) -> bool {
    img["sexual"].as_f64().unwrap_or(2.0) < MAX_SEXUAL
        && img["violence"].as_f64().unwrap_or(2.0) < MAX_VIOLENCE
}

fn parse(v: &serde_json::Value) -> Meta {
    let cover_url = Some(&v["image"])
        .filter(|i| safe(i))
        .and_then(|i| i["url"].as_str())
        .map(String::from);
    // Prefer the first safe, landscape screenshot; it makes the best card.
    let screenshot_url = v["screenshots"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| safe(s))
        .find(|s| s["dims"][0].as_u64().unwrap_or(0) > s["dims"][1].as_u64().unwrap_or(0))
        .and_then(|s| s["url"].as_str())
        .map(String::from);
    Meta {
        id: v["id"].as_str().unwrap_or_default().to_string(),
        title: v["title"].as_str().unwrap_or_default().to_string(),
        english_title: v["titles"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| t["lang"] == "en" && t["official"] == true)
            .and_then(|t| t["title"].as_str().map(String::from)),
        original_title: v["alttitle"].as_str().map(String::from),
        released: v["released"].as_str().map(String::from),
        developers: v["developers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d["name"].as_str().map(String::from))
            .collect(),
        cover_url,
        screenshot_url,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_fuzzy_hits() {
        let protogen = json!({ "title": "My Protogen Engineer", "aliases": [] });
        assert!(!names_match("engine", &protogen));
        let gss = json!({ "title": "Gore Screaming Show", "aliases": ["GSS"] });
        assert!(names_match("gss", &gss));
        let rj = json!({ "title": "Riddle Joker", "titles": [{ "title": "リドル・ジョーカー" }] });
        assert!(names_match(
            "RiddleJokerR18",
            &json!({ "title": "RiddleJoker" })
        ));
        assert!(names_match("Riddle Joker", &rj));
        assert!(names_match(
            "SenrenBanka r18",
            &json!({ "title": "Senren * Banka" })
        ));
    }
}
