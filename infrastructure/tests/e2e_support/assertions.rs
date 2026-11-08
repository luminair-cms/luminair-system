//! RFC 9457 problem details and JSON response assertions for E2E testing.

use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;
use serde_json::Value;

#[allow(unused_imports)]
pub use assert_json_diff::assert_json_include;

// ---------------------------------------------------------------------------
// Public Assertion Functions
// ---------------------------------------------------------------------------

/// Asserts that the response matches the expected status code and returns parsed JSON.
///
/// Prints the response body if the status code does not match.
pub async fn parse_json_with_status(resp: reqwest::Response, expected_status: StatusCode) -> Value {
    let actual_status = resp.status();
    let body_text = resp.text().await.expect("read response body text");
    assert_eq!(
        actual_status, expected_status,
        "Expected HTTP {expected_status}, got {actual_status}. Body: {body_text}"
    );
    serde_json::from_str(&body_text).expect("parse JSON response body")
}

/// Asserts that the response conforms to RFC 9457 `application/problem+json` format.
///
/// Validates HTTP status code, `Content-Type` header, `code` string, and numeric `status`.
/// Returns the parsed JSON `Value` for subsequent custom detail assertions.
pub async fn assert_problem_details(
    resp: reqwest::Response,
    expected_status: StatusCode,
    expected_code: &str,
) -> Value {
    let actual_status = resp.status();
    let content_type = extract_content_type(&resp);
    let body_text = resp.text().await.expect("read problem details body");

    assert_eq!(
        actual_status, expected_status,
        "Expected status {expected_status}, got {actual_status}. Body: {body_text}"
    );
    assert!(
        content_type.starts_with("application/problem+json"),
        "Expected Content-Type to start with 'application/problem+json', got '{content_type}'. Body: {body_text}"
    );

    let body: Value = serde_json::from_str(&body_text).expect("parse RFC 9457 JSON");
    assert_problem_fields(&body, expected_code, expected_status, &body_text);
    body
}

/// Asserts that a response matches the expected status code without consuming the body.
pub fn assert_status(actual: StatusCode, expected: StatusCode) {
    assert_eq!(actual, expected, "Expected status {expected}, got {actual}");
}

/// Extracts the entity ID from an API standard response envelope `{"data": {"id": "..."}}`.
pub fn extract_data_id(body: &Value) -> String {
    body["data"]["id"]
        .as_str()
        .expect("data.id string in response envelope")
        .to_string()
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn extract_content_type(resp: &reqwest::Response) -> String {
    resp.headers()
        .get(CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_string()
}

fn assert_problem_fields(
    body: &Value,
    expected_code: &str,
    expected_status: StatusCode,
    body_text: &str,
) {
    assert_eq!(
        body["code"].as_str(),
        Some(expected_code),
        "Expected problem code '{expected_code}', got {:?}. Body: {body_text}",
        body["code"]
    );
    assert_eq!(
        body["status"].as_u64(),
        Some(u64::from(expected_status.as_u16())),
        "Expected problem status {}, got {:?}. Body: {body_text}",
        expected_status.as_u16(),
        body["status"]
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_data_id() {
        let payload = json!({
            "data": {
                "id": "018f4a12-8812-7000-8000-000000000001",
                "title": "Hello World"
            }
        });
        assert_eq!(
            extract_data_id(&payload),
            "018f4a12-8812-7000-8000-000000000001"
        );
    }

    #[test]
    #[should_panic(expected = "data.id string in response envelope")]
    fn test_extract_data_id_missing() {
        let payload = json!({"data": {}});
        extract_data_id(&payload);
    }

    #[test]
    fn test_assert_status() {
        assert_status(StatusCode::OK, StatusCode::OK);
    }
}
