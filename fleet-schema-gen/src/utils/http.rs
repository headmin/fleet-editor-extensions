use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};

pub fn create_client() -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static("fleet-schema-gen"));

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    Ok(client)
}

pub async fn fetch_url(url: &str) -> Result<String> {
    let client = create_client()?;
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP request failed: {} (status: {})", url, response.status());
    }

    Ok(response.text().await?)
}

pub async fn fetch_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T> {
    let client = create_client()?;
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP request failed: {} (status: {})", url, response.status());
    }

    Ok(response.json().await?)
}
