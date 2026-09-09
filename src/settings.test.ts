import { describe, expect, it } from "vitest";
import { normalizeTimes, validateSettings } from "./settings";

describe("settings", () => {
  it("deduplicates and sorts valid check times", () => {
    expect(normalizeTimes(["22:00", "09:30", "22:00", "25:00"])).toEqual(["09:30", "22:00"]);
  });

  it("requires a positive integer goal and one time", () => {
    expect(validateSettings({ dailyXpGoal: 0, checkTimes: ["20:00"], refreshIntervalMinutes: 30, skipIfCompleted: true, autostart: false })).toContain("XP");
    expect(validateSettings({ dailyXpGoal: 30, checkTimes: [], refreshIntervalMinutes: 30, skipIfCompleted: true, autostart: false })).toContain("时间");
    expect(validateSettings({ dailyXpGoal: 30, checkTimes: ["20:00"], refreshIntervalMinutes: 1, skipIfCompleted: true, autostart: false })).toContain("刷新间隔");
  });
});
