import { useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { normalizeTimes, validateSettings } from "./settings";
import type { AppSettings, CheckResult, DailyStatus } from "./types";

const defaults: AppSettings = {
  dailyXpGoal: 30,
  checkTimes: ["12:00", "18:00", "22:00"],
  skipIfCompleted: true,
  autostart: false,
};

const emptyStatus: DailyStatus = {
  date: new Date().toLocaleDateString("sv-SE"),
  currentXp: 0,
  targetXp: 30,
  completed: false,
  lastSuccessfulCheck: null,
  freshness: "never",
  username: null,
};

function resultMessage(result: CheckResult): string {
  switch (result.kind) {
    case "success": return "状态已更新";
    case "not_authenticated": return "登录已失效，请重新导入会话";
    case "rate_limited": return "请求过于频繁，请稍后再试";
    case "network_error": return "无法连接 Duolingo，请检查网络";
    case "upstream_changed": return "Duolingo 接口可能已变化，请查看日志或更新应用";
  }
}

export default function App() {
  const [tab, setTab] = useState<"today" | "settings">("today");
  const [settings, setSettings] = useState(defaults);
  const [status, setStatus] = useState(emptyStatus);
  const [token, setToken] = useState("");
  const [newTime, setNewTime] = useState("20:00");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    Promise.all([api.getSettings(), api.getStatus()])
      .then(([loadedSettings, loadedStatus]) => {
        setSettings(loadedSettings);
        setStatus(loadedStatus);
      })
      .catch(() => setMessage("应用服务尚未就绪，请稍后重试"));
  }, []);

  const percent = useMemo(
    () => Math.min(100, Math.round((status.currentXp / Math.max(1, status.targetXp)) * 100)),
    [status],
  );

  async function runCheck() {
    setBusy(true);
    try {
      const result = await api.checkNow();
      if (result.kind === "success") setStatus(result.status);
      setMessage(resultMessage(result));
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function saveSettings() {
    const next = { ...settings, checkTimes: normalizeTimes(settings.checkTimes) };
    const validation = validateSettings(next);
    if (validation) return setMessage(validation);
    setBusy(true);
    try {
      const saved = await api.updateSettings(next);
      setSettings(saved);
      setStatus((old) => ({ ...old, targetXp: saved.dailyXpGoal, completed: old.currentXp >= saved.dailyXpGoal }));
      setMessage("设置已保存");
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function importSession() {
    if (!token.trim()) return setMessage("请粘贴 jwt_token 的值");
    setBusy(true);
    try {
      const result = await api.importSession(token.trim());
      setToken("");
      if (result.kind === "success") setStatus(result.status);
      setMessage(resultMessage(result));
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  function addTime() {
    const next = normalizeTimes([...settings.checkTimes, newTime]);
    if (next.length === settings.checkTimes.length) return setMessage("时间无效或已存在");
    setSettings({ ...settings, checkTimes: next });
  }

  async function testNotification() {
    setBusy(true);
    try {
      if (!(await api.ensureNotificationPermission())) {
        setMessage("通知权限未开启，请在 Windows 设置中允许 DuoPing 通知");
        return;
      }
      await api.testNotification();
      setMessage("测试通知已发送，请同时检查 Windows 通知中心");
    } catch (error) {
      setMessage(`发送通知失败：${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="shell">
      <aside className="sidebar">
        <div className="brand"><span className="logo">D</span><div><strong>DuoPing</strong><small>别让今天悄悄溜走</small></div></div>
        <nav>
          <button className={tab === "today" ? "active" : ""} onClick={() => setTab("today")}>今日进度</button>
          <button className={tab === "settings" ? "active" : ""} onClick={() => setTab("settings")}>设置</button>
        </nav>
        <p className="disclaimer">非 Duolingo 官方产品<br />数据仅保存在本机</p>
      </aside>

      <section className="content">
        {tab === "today" ? (
          <>
            <header><div><p className="eyebrow">DAILY FOCUS</p><h1>今天，再向前一点。</h1></div><span className={`status-pill ${status.completed ? "done" : ""}`}>{status.completed ? "今日已完成" : "进行中"}</span></header>
            <article className="hero-card">
              <div className="metric"><span>{status.currentXp}</span><small>/ {status.targetXp} XP</small></div>
              <div className="progress"><i style={{ width: `${percent}%` }} /></div>
              <div className="progress-meta"><span>今日进度 {percent}%</span><span>{status.username ? `@${status.username}` : "尚未连接账号"}</span></div>
              <button className="primary" disabled={busy} onClick={runCheck}>{busy ? "正在检查…" : "立即检查"}</button>
            </article>
            <div className="cards">
              <article><p>下次检查</p><strong>{settings.checkTimes.find((time) => time > new Date().toTimeString().slice(0, 5)) ?? settings.checkTimes[0]}</strong><small>按本机时区</small></article>
              <article><p>最后更新</p><strong>{status.lastSuccessfulCheck ? new Date(status.lastSuccessfulCheck).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : "—"}</strong><small>{status.freshness === "stale" ? "数据可能已过期" : "当天状态"}</small></article>
            </div>
          </>
        ) : (
          <>
            <header><div><p className="eyebrow">PREFERENCES</p><h1>把提醒调成你的节奏。</h1></div></header>
            <section className="settings-card">
              <h2>Duolingo 会话</h2>
              <p className="hint">在已登录的 duolingo.com Cookie 中复制 <code>jwt_token</code> 的值。不会保存账号密码。</p>
              <div className="row"><input type="password" value={token} onChange={(e) => setToken(e.target.value)} placeholder="粘贴 jwt_token" autoComplete="off" /><button onClick={importSession} disabled={busy}>导入并验证</button></div>
              <button className="text-button danger" onClick={() => api.clearSession().then(() => { setStatus(emptyStatus); setMessage("会话已清除"); })}>清除本机会话</button>
            </section>
            <section className="settings-card">
              <h2>每日目标</h2>
              <label>目标 XP<input type="number" min="1" max="10000" value={settings.dailyXpGoal} onChange={(e) => setSettings({ ...settings, dailyXpGoal: Number(e.target.value) })} /></label>
              <h2>检查时间</h2>
              <div className="time-list">{settings.checkTimes.map((time) => <span key={time}>{time}<button aria-label={`删除 ${time}`} onClick={() => setSettings({ ...settings, checkTimes: settings.checkTimes.filter((item) => item !== time) })}>×</button></span>)}</div>
              <div className="row compact"><input type="time" value={newTime} onChange={(e) => setNewTime(e.target.value)} /><button onClick={addTime}>添加时间</button></div>
              <label className="toggle"><input type="checkbox" checked={settings.skipIfCompleted} onChange={(e) => { setMessage(""); setSettings({ ...settings, skipIfCompleted: e.target.checked }); }} />完成目标后停止当天提醒</label>
              <label className="toggle"><input type="checkbox" checked={settings.autostart} onChange={(e) => { setMessage(""); setSettings({ ...settings, autostart: e.target.checked }); }} />开机时自动启动</label>
              <div className="actions"><button className="primary" onClick={saveSettings} disabled={busy}>保存设置</button><button onClick={testNotification} disabled={busy}>测试通知</button></div>
            </section>
          </>
        )}
        {message && <div className="toast" role="status" onClick={() => setMessage("")}>{message}</div>}
      </section>
    </main>
  );
}
