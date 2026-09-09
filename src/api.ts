import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import type { AppSettings, CheckResult, DailyStatus } from "./types";

export const api = {
  getSettings: () => invoke<AppSettings>("get_settings"),
  updateSettings: (settings: AppSettings) => invoke<AppSettings>("update_settings", { settings }),
  getStatus: () => invoke<DailyStatus>("get_status"),
  checkNow: () => invoke<CheckResult>("check_now"),
  importSession: (token: string) => invoke<CheckResult>("import_session", { token }),
  clearSession: () => invoke<void>("clear_session"),
  testNotification: () => invoke<void>("test_notification"),
  ensureNotificationPermission: async () => {
    if (await isPermissionGranted()) return true;
    return (await requestPermission()) === "granted";
  },
};
