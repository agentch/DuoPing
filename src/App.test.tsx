// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const invoke = vi.fn();
const isPermissionGranted = vi.fn();
const requestPermission = vi.fn();
let sessionAvailable = true;
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: () => isPermissionGranted(),
  requestPermission: () => requestPermission(),
}));

describe("App", () => {
  afterEach(cleanup);

  beforeEach(() => {
    sessionAvailable = true;
    isPermissionGranted.mockResolvedValue(true);
    requestPermission.mockResolvedValue("granted");
    invoke.mockImplementation((command: string) => {
      if (command === "get_settings") {
        return Promise.resolve({ dailyXpGoal: 50, checkTimes: ["18:00", "22:00"], refreshIntervalMinutes: 30, skipIfCompleted: true, autostart: false });
      }
      if (command === "get_status") {
        return Promise.resolve({ date: "2026-09-08", currentXp: 20, targetXp: 50, completed: false, lastSuccessfulCheck: null, freshness: "fresh", username: "learner" });
      }
      if (command === "has_session") return Promise.resolve(sessionAvailable);
      return Promise.resolve();
    });
  });

  it("loads and presents today's status from the backend", async () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "今天，再向前一点。" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("@learner")).toBeInTheDocument());
    expect(screen.getByText("/ 50 XP")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("get_settings");
    expect(invoke).toHaveBeenCalledWith("get_status");
  });

  it("hides quest data returned by the backend while the upstream source is unreliable", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_settings") return Promise.resolve({ dailyXpGoal: 50, checkTimes: ["18:00"], refreshIntervalMinutes: 30, skipIfCompleted: true, autostart: false });
      if (command === "get_status") return Promise.resolve({ date: "2026-09-08", currentXp: 20, targetXp: 50, completed: false, lastSuccessfulCheck: null, freshness: "fresh", username: "learner", quests: [{ id: "daily_lessons", title: "完成 3 节课程", current: 2, target: 3, completed: false, kind: "daily" }], questsLastSuccessfulCheck: "2026-09-08T10:00:00Z" });
      if (command === "has_session") return Promise.resolve(true);
      return Promise.resolve();
    });
    render(<App />);
    await screen.findByText("@learner");
    expect(screen.queryByText("每日任务")).not.toBeInTheDocument();
    expect(screen.queryByText("完成 3 节课程")).not.toBeInTheDocument();
  });

  it("reports when notification permission is denied", async () => {
    isPermissionGranted.mockResolvedValue(false);
    requestPermission.mockResolvedValue("denied");
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(screen.getByRole("button", { name: "测试通知" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("通知权限未开启"));
    expect(invoke).not.toHaveBeenCalledWith("test_notification");
  });

  it("opens the embedded Duolingo login window", async () => {
    sessionAvailable = false;
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(await screen.findByRole("button", { name: "登录 Duolingo" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("start_duolingo_login"));
    expect(await screen.findByRole("button", { name: "等待登录…" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "取消登录" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("cancel_duolingo_login"));
    expect(screen.getByRole("button", { name: "登录 Duolingo" })).toBeEnabled();
  });

  it("shows logout after a session is available", async () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "退出登录" })).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "退出登录" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("clear_session"));
    expect(screen.getByRole("button", { name: "登录 Duolingo" })).toBeInTheDocument();
  });

  it("automatically dismisses toast messages", async () => {
    vi.useFakeTimers();
    isPermissionGranted.mockResolvedValue(false);
    requestPermission.mockResolvedValue("denied");
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(screen.getByRole("button", { name: "测试通知" }));
    await act(async () => Promise.resolve());
    expect(screen.getByRole("status")).toHaveTextContent("通知权限未开启");

    act(() => vi.advanceTimersByTime(4_000));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    vi.useRealTimers();
  });
});
