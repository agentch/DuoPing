export type Freshness = "never" | "fresh" | "stale";

export interface AppSettings {
  dailyXpGoal: number;
  checkTimes: string[];
  skipIfCompleted: boolean;
  autostart: boolean;
}

export interface DailyStatus {
  date: string;
  currentXp: number;
  targetXp: number;
  completed: boolean;
  lastSuccessfulCheck: string | null;
  freshness: Freshness;
  username: string | null;
  quests: DailyQuest[];
  questsLastSuccessfulCheck: string | null;
}

export interface DailyQuest {
  id: string;
  title: string;
  current: number;
  target: number;
  completed: boolean;
  kind?: "daily" | "friends" | "monthly";
}

export type CheckResult =
  | { kind: "success"; status: DailyStatus }
  | { kind: "not_authenticated"; message: string }
  | { kind: "rate_limited"; message: string }
  | { kind: "network_error"; message: string }
  | { kind: "upstream_changed"; message: string };
