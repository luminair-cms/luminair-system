//! Ergonomic HTTP and OIDC test client for end-to-end integration testing.

use serde_json::Value;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// HTTP and authentication client for end-to-end service testing.
#[derive(Debug, Clone)]
pub struct E2eClient {
    http: reqwest::Client,
    base_url: String,
    dex_url: String,
    pub service_token: String,
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl E2eClient {
    /// Creates a new test client bound to the given service endpoints.
    pub fn new(base_url: String, dex_url: String, service_token: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url,
            dex_url,
            service_token,
        }
    }

    /// Obtains an RS256 OIDC ID Token from the Dex IDP via resource owner password credentials.
    pub async fn fetch_dex_id_token(&self, email: &str, password: &str) -> String {
        let token_endpoint = format!("{}/token", self.dex_url);
        let form_params = [
            ("grant_type", "password"),
            ("scope", "openid profile email"),
            ("client_id", "luminair-admin-spa"),
            ("username", email),
            ("password", password),
        ];

        let resp = self
            .http
            .post(&token_endpoint)
            .header("Host", "localhost:5556")
            .form(&form_params)
            .send()
            .await
            .expect("execute Dex token request");

        let status = resp.status();
        let body_text = resp.text().await.expect("read Dex token response body");
        assert!(
            status.is_success(),
            "fetch_dex_id_token failed with HTTP {status}: {body_text}"
        );

        let body: Value = serde_json::from_str(&body_text).expect("parse Dex token JSON response");
        body["id_token"]
            .as_str()
            .expect("id_token field present in Dex response")
            .to_string()
    }

    /// Sends an unauthenticated `GET` request to the given path.
    pub async fn get_unauthenticated(&self, path: &str) -> reqwest::Response {
        self.http
            .get(self.format_path(path))
            .send()
            .await
            .expect("execute unauthenticated GET request")
    }

    /// Sends an authenticated `GET` request using the provided bearer token.
    pub async fn get(&self, path: &str, token: &str) -> reqwest::Response {
        self.http
            .get(self.format_path(path))
            .bearer_auth(token)
            .send()
            .await
            .expect("execute GET request")
    }

    /// Sends an authenticated `POST` request with a JSON payload.
    pub async fn post_json(&self, path: &str, token: &str, body: &Value) -> reqwest::Response {
        self.http
            .post(self.format_path(path))
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .expect("execute POST JSON request")
    }

    /// Sends an authenticated `POST` request with an empty body.
    pub async fn post_empty(&self, path: &str, token: &str) -> reqwest::Response {
        self.http
            .post(self.format_path(path))
            .bearer_auth(token)
            .send()
            .await
            .expect("execute POST empty request")
    }

    /// Sends an authenticated `PUT` request with a JSON payload.
    pub async fn put_json(&self, path: &str, token: &str, body: &Value) -> reqwest::Response {
        self.http
            .put(self.format_path(path))
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .expect("execute PUT JSON request")
    }

    /// Sends an authenticated `DELETE` request.
    pub async fn delete(&self, path: &str, token: &str) -> reqwest::Response {
        self.http
            .delete(self.format_path(path))
            .bearer_auth(token)
            .send()
            .await
            .expect("execute DELETE request")
    }

    fn format_path(&self, path: &str) -> String {
        let trimmed = path.strip_prefix('/').unwrap_or(path);
        format!("{}/{}", self.base_url, trimmed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_path() {
        let client = E2eClient::new(
            "http://127.0.0.1:8080".to_string(),
            "http://127.0.0.1:5556/dex".to_string(),
            "token".to_string(),
        );
        assert_eq!(
            client.format_path("healthz"),
            "http://127.0.0.1:8080/healthz"
        );
        assert_eq!(
            client.format_path("/healthz"),
            "http://127.0.0.1:8080/healthz"
        );
    }
}
