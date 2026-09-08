import type { AppSettings } from "./types";

const TIME_PATTERN = /^(?:[01]\d|2[0-3]):[0-5]\d$/;

export function normalizeTimes(times: string[]): string[] {
  return [...new Set(times.map((time) => time.trim()).filter((time) => TIME_PATTERN.test(time)))].sort();
}

export function validateSettings(settings: AppSettings): string | null {
  if (!Number.isInteger(settings.dailyXpGoal) || settings.dailyXpGoal < 1 || settings.dailyXpGoal > 10000) {
    return "每日 XP 目标需为 1–10000 的整数";
  }
  if (normalizeTimes(settings.checkTimes).length === 0) return "请至少添加一个有效检查时间";
  return null;
}

