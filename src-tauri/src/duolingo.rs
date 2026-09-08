use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{NaiveDate, TimeZone, Utc};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct UserSummary {
    pub user_id: String,
    pub username: String,
}

#[derive(Debug, Error, PartialEq)]
pub enum ProviderError {
    #[error("会话无效或已过期")]
    NotAuthenticated,
    #[error("Duolingo 请求过于频繁")]
    RateLimited,
    #[error("无法连接 Duolingo")]
    Network,
    #[error("Duolingo 返回了无法识别的数据")]
    UpstreamChanged,
}

#[async_trait]
pub trait DuolingoProvider: Send + Sync {
    async fn validate_session(&self, token: &str) -> Result<UserSummary, ProviderError>;
    async fn daily_xp(
        &self,
        token: &str,
        user: &UserSummary,
        date: NaiveDate,
    ) -> Result<u32, ProviderError>;
}

pub struct HttpDuolingoProvider {
    client: Client,
}

impl Default for HttpDuolingoProvider {
    fn default() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .user_agent("DuoPing/0.1")
                .build()
                .expect("HTTP client"),
        }
    }
}

#[async_trait]
impl DuolingoProvider for HttpDuolingoProvider {
    async fn validate_session(&self, token: &str) -> Result<UserSummary, ProviderError> {
        let user_id = jwt_subject(token)?;
        let url = format!(
            "https://www.duolingo.com/2017-06-30/users/{}?fields=id,username",
            urlencoding::encode(&user_id)
        );
        let response = self
            .client
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| ProviderError::Network)?;
        classify_status(response.status())?;
        let body: Value = response
            .json()
            .await
            .map_err(|_| ProviderError::UpstreamChanged)?;
        let username = body
            .get("username")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .ok_or(ProviderError::UpstreamChanged)?;
        Ok(UserSummary {
            user_id,
            username: username.into(),
        })
    }

    async fn daily_xp(
        &self,
        token: &str,
        user: &UserSummary,
        date: NaiveDate,
    ) -> Result<u32, ProviderError> {
        let date_text = date.format("%Y-%m-%d").to_string();
        let url = format!(
            "https://www.duolingo.com/2017-06-30/users/{}/xp_summaries?startDate={}&endDate={}",
            urlencoding::encode(&user.user_id),
            date_text,
            date_text
        );
        let response = self
            .client
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| ProviderError::Network)?;
        classify_status(response.status())?;
        let body: Value = response
            .json()
            .await
            .map_err(|_| ProviderError::UpstreamChanged)?;
        parse_daily_xp(&body, date).ok_or(ProviderError::UpstreamChanged)
    }
}

fn classify_status(status: StatusCode) -> Result<(), ProviderError> {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ProviderError::NotAuthenticated),
        StatusCode::TOO_MANY_REQUESTS => Err(ProviderError::RateLimited),
        value if value.is_success() => Ok(()),
        value if value.is_server_error() => Err(ProviderError::Network),
        _ => Err(ProviderError::UpstreamChanged),
    }
}

pub fn jwt_subject(token: &str) -> Result<String, ProviderError> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or(ProviderError::NotAuthenticated)?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| ProviderError::NotAuthenticated)?;
    let value: Value =
        serde_json::from_slice(&decoded).map_err(|_| ProviderError::NotAuthenticated)?;
    match value.get("sub") {
        Some(Value::String(value)) if !value.is_empty() => Ok(value.clone()),
        Some(Value::Number(value)) => Ok(value.to_string()),
        _ => Err(ProviderError::NotAuthenticated),
    }
}

fn parse_daily_xp(body: &Value, target: NaiveDate) -> Option<u32> {
    let rows = body
        .get("summaries")
        .or_else(|| body.get("xpSummaries"))
        .or_else(|| body.as_array().map(|_| body))?
        .as_array()?;
    for row in rows {
        let matches = match row.get("date")? {
            Value::String(value) => value.starts_with(&target.format("%Y-%m-%d").to_string()),
            Value::Number(value) => value
                .as_i64()
                .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single())
                .is_some_and(|date| date.date_naive() == target),
            _ => false,
        };
        if matches {
            return row
                .get("gainedXp")
                .or_else(|| row.get("xp"))?
                .as_u64()
                .and_then(|xp| u32::try_from(xp).ok());
        }
    }
    // A successful empty result means no XP, not a schema failure.
    if rows.is_empty() {
        Some(0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_string_and_numeric_jwt_subjects() {
        assert_eq!(jwt_subject("x.eyJzdWIiOiIxMjMifQ.y").unwrap(), "123");
        assert_eq!(jwt_subject("x.eyJzdWIiOjEyM30.y").unwrap(), "123");
        assert_eq!(jwt_subject("broken"), Err(ProviderError::NotAuthenticated));
    }

    #[test]
    fn parses_supported_xp_summary_shapes() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 8).unwrap();
        assert_eq!(
            parse_daily_xp(
                &json!({"summaries":[{"date":1788825600_i64,"gainedXp":42}]}),
                date
            ),
            Some(42)
        );
        assert_eq!(
            parse_daily_xp(&json!({"xpSummaries":[{"date":"2026-09-08","xp":9}]}), date),
            Some(9)
        );
        assert_eq!(parse_daily_xp(&json!({"summaries":[]}), date), Some(0));
        assert_eq!(parse_daily_xp(&json!({"unexpected":[]}), date), None);
    }
}
