use anyhow::Result;
use reqwest::Client;

pub struct ApiClient {
    client: Client,
    base_url: String,
}

impl ApiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}/api{}", self.base_url, path);
        let resp = self.client.get(&url).send().await?;
        let body = resp.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn post<B: serde::Serialize, T: serde::de::DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}/api{}", self.base_url, path);
        let resp = self.client.post(&url).json(body).send().await?;
        let body = resp.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn put<B: serde::Serialize, T: serde::de::DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}/api{}", self.base_url, path);
        let resp = self.client.put(&url).json(body).send().await?;
        let body = resp.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn delete<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}/api{}", self.base_url, path);
        let resp = self.client.delete(&url).send().await?;
        let body = resp.text().await?;
        Ok(serde_json::from_str(&body)?)
    }
}
