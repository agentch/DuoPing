// @vitest-environment jsdom
import { render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

describe("App", () => {
  beforeEach(() => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_settings") {
        return Promise.resolve({ dailyXpGoal: 50, checkTimes: ["18:00", "22:00"], skipIfCompleted: true, autostart: false });
      }
      if (command === "get_status") {
        return Promise.resolve({ date: "2026-09-08", currentXp: 20, targetXp: 50, completed: false, lastSuccessfulCheck: null, freshness: "fresh", username: "learner" });
      }
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
});
