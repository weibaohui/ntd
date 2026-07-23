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
            // 末尾斜杠统一剥掉，避免拼路径时出现 // 造成 404
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    // 统一 URL 拼接：base + /api/v1 + path。
    // ADR-7 之后所有业务端点都挂在 /api/v1 下，调用方传入相对路径（如 "/todos"、
    // "/workspaces/1/todos/2"），由这里统一前缀化，避免每个调用点重复写 v1。
    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base_url, path)
    }

    pub async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path);
        let resp = self.client.get(&url).send().await?;
        let body = resp.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn post<B: serde::Serialize, T: serde::de::DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let url = self.url(path);
        let resp = self.client.post(&url).json(body).send().await?;
        let body = resp.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn put<B: serde::Serialize, T: serde::de::DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let url = self.url(path);
        let resp = self.client.put(&url).json(body).send().await?;
        let body = resp.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn delete<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path);
        let resp = self.client.delete(&url).send().await?;
        let body = resp.text().await?;
        Ok(serde_json::from_str(&body)?)
    }
}
