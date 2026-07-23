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
    // 防御性补上前导斜杠：调用方若漏写，避免拼出 /api/v1todos 这种非法路径。
    fn url(&self, path: &str) -> String {
        // 调用方传入的路径必须以 / 开头；若遗漏则自动补 /，避免拼出 /api/v1todos。
        if path.starts_with('/') {
            format!("{}/api/v1{}", self.base_url, path)
        } else {
            format!("{}/api/v1/{}", self.base_url, path)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// base_url 带尾部斜杠、path 带前导斜杠：正确剥斜杠并拼出 v1 路径。
    #[test]
    fn test_url_with_trailing_slash_and_leading_slash() {
        let client = ApiClient::new("http://localhost:18088/");
        assert_eq!(client.url("/todos"), "http://localhost:18088/api/v1/todos");
    }

    /// base_url 无尾部斜杠、path 带前导斜杠：正常拼接。
    #[test]
    fn test_url_without_trailing_slash() {
        let client = ApiClient::new("http://localhost:18088");
        assert_eq!(
            client.url("/workspaces/1/todos/2"),
            "http://localhost:18088/api/v1/workspaces/1/todos/2"
        );
    }

    /// path 漏写前导斜杠：防御性补 /，避免 /api/v1todos。
    #[test]
    fn test_url_without_leading_slash() {
        let client = ApiClient::new("http://localhost:18088");
        assert_eq!(client.url("todos"), "http://localhost:18088/api/v1/todos");
    }
}
