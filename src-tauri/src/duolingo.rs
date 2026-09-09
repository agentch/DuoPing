use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{NaiveDate, TimeZone, Utc};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use thiserror::Error;

use crate::model::DailyQuest;

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
    async fn daily_quests(
        &self,
        token: &str,
        user: &UserSummary,
        timezone: &str,
    ) -> Result<Vec<DailyQuest>, ProviderError>;
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

    async fn daily_quests(
        &self,
        token: &str,
        user: &UserSummary,
        timezone: &str,
    ) -> Result<Vec<DailyQuest>, ProviderError> {
        let headers = |request: reqwest::RequestBuilder| {
            request
                .bearer_auth(token)
                .header("x-requested-with", "XMLHttpRequest")
                .header("accept", "application/json; charset=UTF-8")
        };
        let schema = headers(
            self.client
                .get("https://goals-api.duolingo.com/schema?ui_language=en"),
        )
        .send()
        .await
        .map_err(|_| ProviderError::Network)?;
        classify_status(schema.status())?;
        let schema: Value = schema
            .json()
            .await
            .map_err(|_| ProviderError::UpstreamChanged)?;
        let progress_url = format!(
            "https://goals-api.duolingo.com/users/{}/progress?timezone={}&ui_language=en",
            urlencoding::encode(&user.user_id),
            urlencoding::encode(timezone)
        );
        let progress = headers(self.client.get(progress_url))
            .send()
            .await
            .map_err(|_| ProviderError::Network)?;
        classify_status(progress.status())?;
        let progress: Value = progress
            .json()
            .await
            .map_err(|_| ProviderError::UpstreamChanged)?;
        parse_daily_quests(&schema, &progress).ok_or(ProviderError::UpstreamChanged)
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

fn parse_daily_quests(schema: &Value, progress: &Value) -> Option<Vec<DailyQuest>> {
    let goals = schema.get("goals")?.as_array()?;
    let earned = progress
        .get("badges")
        .and_then(|badges| badges.get("earned"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let values = progress
        .get("goals")
        .and_then(|goals| goals.get("progress"))
        .and_then(Value::as_object);

    goals
        .iter()
        .filter(|goal| is_daily_goal(goal.get("category")))
        .filter(|goal| {
            goal.get("goalId")
                .and_then(Value::as_str)
                .is_some_and(|goal_id| values.is_some_and(|values| values.contains_key(goal_id)))
        })
        .filter_map(|goal| {
            let id = goal.get("goalId")?.as_str()?.to_owned();
            let badge_id = goal.get("badgeId").and_then(Value::as_str);
            let title = goal
                .get("title")
                .and_then(|title| title.get("uiString"))
                .and_then(Value::as_str)
                .or_else(|| goal.get("title").and_then(Value::as_str))?
                .to_owned();
            let target = goal
                .get("threshold")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(1)
                .max(1);
            let raw_progress = values.and_then(|values| {
                values.get(&id).or_else(|| {
                    goal.get("metric")
                        .and_then(Value::as_str)
                        .and_then(|metric| values.get(metric))
                })
            });
            let mut current = raw_progress
                .and_then(|value| {
                    value
                        .as_u64()
                        .or_else(|| value.get("progress").and_then(Value::as_u64))
                })
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(0)
                .min(target);
            let completed = current >= target
                || earned.contains(id.as_str())
                || badge_id.is_some_and(|value| earned.contains(value));
            if completed {
                current = target;
            }
            Some(DailyQuest {
                id,
                title,
                current,
                target,
                completed,
            })
        })
        .collect::<Vec<_>>()
        .into()
}

fn is_daily_goal(category: Option<&Value>) -> bool {
    let categories = match category {
        Some(Value::String(value)) => vec![value.as_str()],
        Some(Value::Array(values)) => values.iter().filter_map(Value::as_str).collect(),
        _ => return false,
    };
    categories.iter().any(|value| value.contains("DAILY"))
        && !categories.iter().any(|value| value.contains("MONTHLY"))
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

    #[test]
    fn parses_daily_quests_with_partial_and_completed_progress() {
        let schema = json!({"goals":[
            {"goalId":"daily_lessons","badgeId":"badge_lessons","category":["DAILY"],"metric":"LESSONS","threshold":3,"title":{"uiString":"Complete 3 lessons"}},
            {"goalId":"daily_xp","badgeId":"badge_xp","category":["DAILY"],"metric":"XP","threshold":50,"title":{"uiString":"Earn 50 XP"}},
            {"goalId":"monthly","category":["DAILY","MONTHLY"],"threshold":1,"title":{"uiString":"Ignore"}},
            {"goalId":"inactive_daily","category":["DAILY"],"threshold":1,"title":{"uiString":"Inactive"}}
        ]});
        let progress = json!({"goals":{"progress":{"daily_lessons":{"progress":2},"daily_xp":12}},"badges":{"earned":["badge_xp"]}});
        assert_eq!(
            parse_daily_quests(&schema, &progress),
            Some(vec![
                DailyQuest {
                    id: "daily_lessons".into(),
                    title: "Complete 3 lessons".into(),
                    current: 2,
                    target: 3,
                    completed: false
                },
                DailyQuest {
                    id: "daily_xp".into(),
                    title: "Earn 50 XP".into(),
                    current: 50,
                    target: 50,
                    completed: true
                },
            ])
        );
    }

    #[test]
    fn rejects_unrecognized_daily_quest_shapes() {
        assert_eq!(
            parse_daily_quests(&json!({"goals": []}), &json!({"goals": {"progress": {}}})),
            Some(vec![])
        );
        assert_eq!(
            parse_daily_quests(&json!({"goals": [{}]}), &json!({"goals": {"progress": {}}})),
            Some(vec![])
        );
        assert_eq!(
            parse_daily_quests(&json!({"goals": []}), &json!({})),
            Some(vec![])
        );
        assert_eq!(parse_daily_quests(&json!({}), &json!({})), None);
    }

    #[test]
    fn accepts_string_or_array_daily_categories() {
        assert!(is_daily_goal(Some(&json!("DAILY"))));
        assert!(is_daily_goal(Some(&json!("DAILY_QUEST"))));
        assert!(is_daily_goal(Some(&json!(["DAILY", "CHALLENGE"]))));
        assert!(is_daily_goal(Some(&json!(["ACTIVE", "DAILY_QUEST"]))));
        assert!(!is_daily_goal(Some(&json!("MONTHLY"))));
        assert!(!is_daily_goal(Some(&json!(["DAILY", "MONTHLY"]))));
        assert!(!is_daily_goal(Some(&json!("MONTHLY_DAILY_QUEST"))));
    }

    #[test]
    fn ignores_catalog_quests_when_progress_map_is_omitted() {
        let schema = json!({"goals":[
            {"goalId":"daily_xp","badgeId":"badge_xp","category":"DAILY_QUEST","threshold":50,"title":{"uiString":"Earn 50 XP"}}
        ]});
        let progress = json!({"badges":{"earned":["badge_xp"]}});
        assert_eq!(parse_daily_quests(&schema, &progress), Some(vec![]));
    }
}
