use chrono::{DateTime, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub daily_xp_goal: u32,
    pub check_times: Vec<String>,
    pub skip_if_completed: bool,
    pub autostart: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            daily_xp_goal: 30,
            check_times: vec!["12:00".into(), "18:00".into(), "22:00".into()],
            skip_if_completed: true,
            autostart: false,
        }
    }
}

impl AppSettings {
    pub fn normalize(mut self) -> Result<Self, String> {
        if !(1..=10_000).contains(&self.daily_xp_goal) {
            return Err("每日 XP 目标需为 1–10000 的整数".into());
        }
        self.check_times.retain(|value| valid_time(value));
        self.check_times.sort();
        self.check_times.dedup();
        if self.check_times.is_empty() {
            return Err("请至少添加一个有效检查时间".into());
        }
        Ok(self)
    }
}

fn valid_time(value: &str) -> bool {
    let Some((hours, minutes)) = value.split_once(':') else {
        return false;
    };
    hours.len() == 2
        && minutes.len() == 2
        && hours.parse::<u8>().is_ok_and(|hour| hour < 24)
        && minutes.parse::<u8>().is_ok_and(|minute| minute < 60)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DailyStatus {
    pub date: NaiveDate,
    pub current_xp: u32,
    pub target_xp: u32,
    pub completed: bool,
    pub last_successful_check: Option<DateTime<Utc>>,
    pub freshness: Freshness,
    pub username: Option<String>,
    #[serde(default)]
    pub quests: Vec<DailyQuest>,
    #[serde(default)]
    pub quests_last_successful_check: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DailyQuest {
    pub id: String,
    pub title: String,
    pub current: u32,
    pub target: u32,
    pub completed: bool,
    #[serde(default)]
    pub kind: QuestKind,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QuestKind {
    #[default]
    Daily,
    Friends,
    Monthly,
}

impl DailyStatus {
    pub fn empty(target_xp: u32) -> Self {
        Self {
            date: Local::now().date_naive(),
            current_xp: 0,
            target_xp,
            completed: false,
            last_successful_check: None,
            freshness: Freshness::Never,
            username: None,
            quests: vec![],
            quests_last_successful_check: None,
        }
    }

    pub fn roll_to_today(&mut self) {
        let today = Local::now().date_naive();
        if self.date != today {
            *self = Self::empty(self.target_xp);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    Never,
    Fresh,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckResult {
    Success { status: DailyStatus },
    NotAuthenticated { message: String },
    RateLimited { message: String },
    NetworkError { message: String },
    UpstreamChanged { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_sort_and_deduplicate_times() {
        let value = AppSettings {
            check_times: vec![
                "22:00".into(),
                "09:30".into(),
                "22:00".into(),
                "25:00".into(),
            ],
            ..Default::default()
        };
        assert_eq!(
            value.normalize().unwrap().check_times,
            vec!["09:30", "22:00"]
        );
    }

    #[test]
    fn settings_require_goal_and_time() {
        assert!(AppSettings {
            daily_xp_goal: 0,
            ..Default::default()
        }
        .normalize()
        .is_err());
        assert!(AppSettings {
            check_times: vec![],
            ..Default::default()
        }
        .normalize()
        .is_err());
    }
}
