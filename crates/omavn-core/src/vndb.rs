//! Metadata and art from VNDB (https://vndb.org), via its public API.

use anyhow::{Context, Result};
use serde_json::json;

const API: &str = "https://api.vndb.org/kana/vn";
const FIELDS: &str = "id,title,alttitle,released,votecount,developers.name,\
    image.url,image.sexual,image.violence,\
    screenshots.url,screenshots.sexual,screenshots.violence,screenshots.dims";

/// VNDB rates images 0 (safe) to 2 (explicit). Library art stays below this.
const MAX_SEXUAL: f64 = 1.0;
const MAX_VIOLENCE: f64 = 1.0;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Meta {
    pub id: String,
    pub title: String,
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
    let hits = query(json!(["search", "=", title]), 5)?;
    Ok(hits
        .iter()
        .max_by_key(|h| h["votecount"].as_u64().unwrap_or(0))
        .map(parse))
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
