use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use thiserror::Error;

use crate::model::{DailyQuest, QuestKind};

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
    #[allow(dead_code)] // Retained while the unreliable quest feature is paused.
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
                .get("https://goals-api.duolingo.com/schema?ui_language=zh"),
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
            "https://goals-api.duolingo.com/users/{}/progress?timezone={}&ui_language=zh",
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
        parse_quests(&schema, &progress, chrono::Local::now().date_naive())
            .ok_or(ProviderError::UpstreamChanged)
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

#[allow(dead_code)] // Retained while the unreliable quest feature is paused.
fn parse_quests(schema: &Value, progress: &Value, today: NaiveDate) -> Option<Vec<DailyQuest>> {
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
        .filter(|goal| {
            goal.get("goalId")
                .and_then(Value::as_str)
                .is_some_and(|goal_id| values.is_some_and(|values| values.contains_key(goal_id)))
        })
        .filter_map(|goal| {
            let badge_id = goal.get("badgeId").and_then(Value::as_str);
            let id = goal.get("goalId")?.as_str()?.to_owned();
            let title = goal
                .get("title")
                .and_then(|title| title.get("uiString"))
                .and_then(Value::as_str)
                .or_else(|| goal.get("title").and_then(Value::as_str))?
                .to_owned();
            let kind = quest_kind(goal.get("category"), &id, badge_id, &title, today)?;
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
                kind,
            })
        })
        .collect::<Vec<_>>()
        .into()
}

#[allow(dead_code)] // Used by the paused quest parser.
fn categories(category: Option<&Value>) -> Vec<&str> {
    match category {
        Some(Value::String(value)) => vec![value.as_str()],
        Some(Value::Array(values)) => values.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    }
}

#[allow(dead_code)] // Used by the paused quest parser.
fn quest_kind(
    category: Option<&Value>,
    goal_id: &str,
    badge_id: Option<&str>,
    title: &str,
    today: NaiveDate,
) -> Option<QuestKind> {
    let categories = categories(category);
    if categories.iter().any(|value| value.contains("FRIEND")) {
        return Some(QuestKind::Friends);
    }
    let dated_id = [Some(goal_id), badge_id]
        .into_iter()
        .flatten()
        .find(|value| is_dated_goal_id(value));
    if let Some(dated_id) = dated_id {
        let current_month = today.format("%Y_%m").to_string();
        return dated_id
            .starts_with(&current_month)
            .then_some(QuestKind::Monthly);
    }
    if let Some(month) = month_from_title(title) {
        return (month == today.month()).then_some(QuestKind::Monthly);
    }
    if categories.iter().any(|value| value.contains("MONTHLY")) {
        return Some(QuestKind::Monthly);
    }
    categories
        .iter()
        .any(|value| value.contains("DAILY"))
        .then_some(QuestKind::Daily)
}

#[allow(dead_code)] // Used by the paused quest parser.
fn month_from_title(title: &str) -> Option<u32> {
    let compact = title
        .chars()
        .filter(|value| !value.is_whitespace())
        .collect::<String>();
    if let Some(month_marker) = compact.find('月') {
        let digits = compact[..month_marker]
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        if let Ok(month) = digits.parse::<u32>() {
            if (1..=12).contains(&month) {
                return Some(month);
            }
        }
    }

    const ENGLISH_MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    let lowercase = title.to_ascii_lowercase();
    let words = lowercase
        .split(|value: char| !value.is_ascii_alphabetic())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    ENGLISH_MONTHS
        .iter()
        .position(|month| words.contains(month))
        .map(|index| index as u32 + 1)
}

#[allow(dead_code)] // Used by the paused quest parser.
fn is_dated_goal_id(goal_id: &str) -> bool {
    let mut parts = goal_id.split('_');
    let (Some(year), Some(month), Some(_remainder)) = (parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    year.len() == 4
        && year.bytes().all(|value| value.is_ascii_digit())
        && month.len() == 2
        && month
            .parse::<u8>()
            .is_ok_and(|value| (1..=12).contains(&value))
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
    fn parses_active_daily_friend_and_current_month_quests() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 8).unwrap();
        let schema = json!({"goals":[
            {"goalId":"daily_lessons","badgeId":"badge_lessons","category":["DAILY"],"metric":"LESSONS","threshold":3,"title":{"uiString":"Complete 3 lessons"}},
            {"goalId":"daily_xp","badgeId":"badge_xp","category":["DAILY"],"metric":"XP","threshold":50,"title":{"uiString":"Earn 50 XP"}},
            {"goalId":"friends_xp","category":"FRIENDS_QUESTS","threshold":500,"title":{"uiString":"Earn 500 XP with a friend"}},
            {"goalId":"2026_09_monthly_challenge","category":["DAILY","MONTHLY"],"threshold":50,"title":{"uiString":"September Quest"}},
            {"goalId":"2026_08_monthly_challenge","category":["DAILY","MONTHLY"],"threshold":50,"title":{"uiString":"August Quest"}},
            {"goalId":"daily_lessons","badgeId":"2021_03_xp_challenge","category":["DAILY"],"threshold":1,"title":{"uiString":"March XP Challenge"}},
            {"goalId":"daily_lessons","badgeId":"generic_monthly_badge","category":["DAILY"],"threshold":1,"title":{"uiString":"3 月经验挑战"}},
            {"goalId":"inactive_daily","category":["DAILY"],"threshold":1,"title":{"uiString":"Inactive"}}
        ]});
        let progress = json!({"goals":{"progress":{"daily_lessons":{"progress":2},"daily_xp":12,"friends_xp":250,"2026_09_monthly_challenge":21,"2026_08_monthly_challenge":50}},"badges":{"earned":["badge_xp"]}});
        assert_eq!(
            parse_quests(&schema, &progress, today),
            Some(vec![
                DailyQuest {
                    id: "daily_lessons".into(),
                    title: "Complete 3 lessons".into(),
                    current: 2,
                    target: 3,
                    completed: false,
                    kind: QuestKind::Daily,
                },
                DailyQuest {
                    id: "daily_xp".into(),
                    title: "Earn 50 XP".into(),
                    current: 50,
                    target: 50,
                    completed: true,
                    kind: QuestKind::Daily,
                },
                DailyQuest {
                    id: "friends_xp".into(),
                    title: "Earn 500 XP with a friend".into(),
                    current: 250,
                    target: 500,
                    completed: false,
                    kind: QuestKind::Friends,
                },
                DailyQuest {
                    id: "2026_09_monthly_challenge".into(),
                    title: "September Quest".into(),
                    current: 21,
                    target: 50,
                    completed: false,
                    kind: QuestKind::Monthly,
                },
            ])
        );
    }

    #[test]
    fn rejects_unrecognized_daily_quest_shapes() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 8).unwrap();
        assert_eq!(
            parse_quests(
                &json!({"goals": []}),
                &json!({"goals": {"progress": {}}}),
                today
            ),
            Some(vec![])
        );
        assert_eq!(
            parse_quests(
                &json!({"goals": [{}]}),
                &json!({"goals": {"progress": {}}}),
                today
            ),
            Some(vec![])
        );
        assert_eq!(
            parse_quests(&json!({"goals": []}), &json!({}), today),
            Some(vec![])
        );
        assert_eq!(parse_quests(&json!({}), &json!({}), today), None);
    }

    #[test]
    fn classifies_quest_categories_and_current_month() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 8).unwrap();
        assert_eq!(
            quest_kind(Some(&json!("DAILY_QUEST")), "daily", None, "Earn XP", today),
            Some(QuestKind::Daily)
        );
        assert_eq!(
            quest_kind(
                Some(&json!("FRIENDS_QUESTS")),
                "friends",
                None,
                "Earn XP together",
                today
            ),
            Some(QuestKind::Friends)
        );
        assert_eq!(
            quest_kind(
                Some(&json!(["DAILY", "MONTHLY"])),
                "2026_09_monthly",
                None,
                "September Challenge",
                today
            ),
            Some(QuestKind::Monthly)
        );
        assert_eq!(
            quest_kind(
                Some(&json!("MONTHLY_DAILY_QUEST")),
                "2026_08_monthly",
                None,
                "August Challenge",
                today
            ),
            None
        );
        assert_eq!(
            quest_kind(
                Some(&json!("DAILY")),
                "2021_03_monthly_xp_challenge",
                None,
                "March XP Challenge",
                today
            ),
            None
        );
        assert_eq!(
            quest_kind(
                Some(&json!("DAILY")),
                "2021_03_xp_challenge",
                None,
                "March XP Challenge",
                today
            ),
            None
        );
        assert_eq!(
            quest_kind(
                Some(&json!("DAILY")),
                "2026_09_monthly_challenge",
                None,
                "September Challenge",
                today
            ),
            Some(QuestKind::Monthly)
        );
        assert_eq!(
            quest_kind(
                Some(&json!("DAILY")),
                "daily_goal_daily_quest",
                Some("2021_03_monthly_xp_challenge"),
                "March XP Challenge",
                today
            ),
            None
        );
        assert_eq!(
            quest_kind(
                Some(&json!("DAILY")),
                "daily_goal_starter_daily_quest",
                Some("2026_09_monthly_challenge"),
                "September Challenge",
                today
            ),
            Some(QuestKind::Monthly)
        );
        assert_eq!(
            quest_kind(
                Some(&json!("DAILY")),
                "daily_goal_daily_quest",
                Some("generic_badge"),
                "3 月经验挑战",
                today
            ),
            None
        );
        assert_eq!(
            quest_kind(
                Some(&json!("DAILY")),
                "daily_goal_daily_quest",
                Some("generic_badge"),
                "9 月经验挑战",
                today
            ),
            Some(QuestKind::Monthly)
        );
        assert!(!is_dated_goal_id("daily_xp"));
        assert!(!is_dated_goal_id("2026_13_monthly_challenge"));
        assert!(is_dated_goal_id("2021_03_xp_challenge"));
        assert_eq!(month_from_title("3 月经验挑战"), Some(3));
        assert_eq!(month_from_title("September XP Challenge"), Some(9));
        assert_eq!(month_from_title("获取 50 经验"), None);
    }

    #[test]
    fn ignores_catalog_quests_when_progress_map_is_omitted() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 8).unwrap();
        let schema = json!({"goals":[
            {"goalId":"daily_xp","badgeId":"badge_xp","category":"DAILY_QUEST","threshold":50,"title":{"uiString":"Earn 50 XP"}}
        ]});
        let progress = json!({"badges":{"earned":["badge_xp"]}});
        assert_eq!(parse_quests(&schema, &progress, today), Some(vec![]));
    }
}
