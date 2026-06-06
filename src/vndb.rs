use serde::{Deserialize, Serialize};
use std::time::Duration;

///vn result returned from VNDB
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VndbSearchResult {
    pub id: String,
    pub title: String,

    pub image: Option<VndbImage>,
    pub description: Option<String>,
    pub released: Option<String>,
}

///vndb cover info
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VndbImage {
    pub url: Option<String>,
}

///vndb sends this back as a json
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct VndbSearchResponse {
    pub results: Vec<VndbSearchResult>,
}

///the json kakera will send to vndb
#[derive(Debug, Clone, Serialize)]
struct VndbSearchRequest {
    pub filters: serde_json::Value,
    pub fields: String,
    pub sort: String,
    pub results: u32,
}

///searches vndb for vns matching the query
pub async fn search_vns(query: String) -> Result<Vec<VndbSearchResult>, reqwest::Error> {
    let client = reqwest::Client::new();

    tokio::time::sleep(Duration::from_millis(300)).await;

    let request_body = VndbSearchRequest {
        filters: serde_json::json!(["search", "=", query]),
        fields: "id, title, image.url, description, released".to_string(),
        sort: "searchrank".to_string(),
        results: 20,
    };

    let response = client
        .post("https://api.vndb.org/kana/vn")
        .json(&request_body)
        .send()
        .await?
        .error_for_status()?
        .json::<VndbSearchResponse>()
        .await?;

    Ok(response.results)
}
