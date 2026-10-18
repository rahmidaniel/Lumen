import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Bug, ClipboardCopy, Download, FolderOpen, Power, RefreshCw, ShieldAlert, ShieldCheck } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { Badge, Button, Field, Panel, Select, Toggle, cx } from "../../components/ui";
import { isTauri } from "../../lib/ipc";
import type { PowerSettings } from "../../lib/types";
import { useStore } from "../../store";

const ISSUES_URL = "https://github.com/rahmidaniel/lumen/issues/new/choose";
const mac = navigator.platform.startsWith("Mac");
const windows = navigator.platform.startsWith("Win");

function Section({ title, aside, children }: { title: string; aside?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-3 rounded-lg border border-border bg-surface-2 p-3">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">{title}</span>
        {aside}
      </div>
      {children}
    </div>
  );
}

/** A setting with a switch; the whole row is the click target. */
function ToggleRow({ title, hint, checked, onChange, disabled }: { title: string; hint?: string; checked: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <label className={cx("flex items-center justify-between gap-4", disabled ? "opacity-50" : "cursor-pointer")}>
      <span className="flex min-w-0 flex-col gap-0.5">
        <span className="text-sm">{title}</span>
        {hint && <span className="text-xs leading-snug text-muted">{hint}</span>}
      </span>
      <Toggle checked={checked} onChange={onChange} label={title} disabled={disabled} />
    </label>
  );
}

function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="rounded border border-border-strong bg-surface-3 px-1.5 py-0.5 font-mono text-[10px] text-text">{children}</kbd>;
}

function StartupSection() {
  // Autostart lives in the OS (registry / LaunchAgent), not in our store.
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!isTauri) return;
    isEnabled()
      .then(setAutostart)
      .catch(() => setAutostart(null));
  }, []);
  const toggle = async (on: boolean) => {
    try {
      if (on) await enable();
      else await disable();
      setAutostart(on);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };
  return (
    <Section title="Startup">
      <ToggleRow
        title={mac ? "Start with macOS" : windows ? "Start with Windows" : "Start at login"}
        hint="Launches hidden in the tray and lights your devices as soon as you log in."
        checked={autostart === true}
        onChange={toggle}
      />
      {autostart === null && isTauri && <p className="text-xs text-faint">Autostart state unavailable.</p>}
      {error && <p className="text-xs text-danger">{error}</p>}
    </Section>
  );
}

const idleOptions = [
  { value: "0", label: "Never" },
  { value: "5", label: "After 5 minutes" },
  { value: "10", label: "After 10 minutes" },
  { value: "15", label: "After 15 minutes" },
  { value: "30", label: "After 30 minutes" },
  { value: "60", label: "After 1 hour" },
];

function LightsSection() {
  const app = useStore((s) => s.app);
  const setPowerSettings = useStore((s) => s.setPowerSettings);
  const lightsOff = useStore((s) => s.snapshot?.blackout === "user");
  const setLightsOff = useStore((s) => s.setLightsOff);
  const power: PowerSettings = app?.power ?? { sleep: true, display_off: true, lock: false, idle_minutes: 0 };
  const supported = app?.power_supported ?? true;
  const patch = (p: Partial<PowerSettings>) => setPowerSettings({ ...power, ...p });
  return (
    <Section
      title="Lights"
      aside={
        <Button size="sm" variant={lightsOff ? "primary" : "default"} onClick={() => setLightsOff(!lightsOff)}>
          <Power size={14} /> {lightsOff ? "Turn on" : "Turn off"}
        </Button>
      }
    >
      <p className="text-xs text-muted">Turn every device dark while the PC isn't in use, and back on when it is.</p>
      {!supported && <p className="text-xs text-warn">Following the PC's state is only available on Windows so far.</p>}
      <ToggleRow title="When the PC sleeps or shuts down" checked={power.sleep} onChange={(v) => patch({ sleep: v })} disabled={!supported} />
      <ToggleRow title="When the screen turns off" checked={power.display_off} onChange={(v) => patch({ display_off: v })} disabled={!supported} />
      <ToggleRow title="When the PC is locked" checked={power.lock} onChange={(v) => patch({ lock: v })} disabled={!supported} />
      <Field label="When nobody uses the PC">
        <Select value={String(power.idle_minutes)} options={idleOptions} onChange={(v) => patch({ idle_minutes: parseInt(v, 10) })} />
      </Field>
    </Section>
  );
}

function AlertsSection() {
  const app = useStore((s) => s.app);
  const setBatteryAlerts = useStore((s) => s.setBatteryAlerts);
  const setShortcuts = useStore((s) => s.setShortcuts);
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const mods = mac ? "⌃ ⌥ ⇧" : "Ctrl Alt Shift";
  return (
    <Section title="Alerts and shortcuts">
      <ToggleRow
        title="Low-battery notifications"
        hint="A notification when a wireless device drops to 20 %, and again at 5 %."
        checked={app?.battery_alerts ?? true}
        onChange={setBatteryAlerts}
      />
      <ToggleRow
        title="Keyboard shortcuts"
        hint="Work everywhere, even with Lumen closed to the tray."
        checked={app?.shortcuts ?? false}
        onChange={async (v) => setShortcutError(await setShortcuts(v))}
      />
      <ul className="flex flex-col gap-1.5 text-xs text-muted">
        <li className="flex items-center justify-between gap-2">
          <span>Lights on/off</span>
          <span className="flex gap-1">
            <Kbd>{mods}</Kbd>
            <Kbd>L</Kbd>
          </span>
        </li>
        <li className="flex items-center justify-between gap-2">
          <span>Brighter / dimmer</span>
          <span className="flex gap-1">
            <Kbd>{mods}</Kbd>
            <Kbd>↑</Kbd>
            <Kbd>↓</Kbd>
          </span>
        </li>
        <li className="flex items-center justify-between gap-2">
          <span>Next / previous profile</span>
          <span className="flex gap-1">
            <Kbd>{mods}</Kbd>
            <Kbd>→</Kbd>
            <Kbd>←</Kbd>
          </span>
        </li>
      </ul>
      {shortcutError && <p className="text-xs text-danger">{shortcutError}</p>}
    </Section>
  );
}

function OpenRgbSection() {
  const openrgb = useStore((s) => s.openrgb);
  const snapshot = useStore((s) => s.snapshot);
  const error = useStore((s) => s.error);
  const startOpenrgb = useStore((s) => s.startOpenrgb);
  const restartOpenrgb = useStore((s) => s.restartOpenrgb);
  const adoptOpenrgb = useStore((s) => s.adoptOpenrgb);
  const releaseOpenrgb = useStore((s) => s.releaseOpenrgb);
  const refresh = useStore((s) => s.refreshDevices);
  const busy = useStore((s) => s.openrgbBusy);
  const owns = openrgb?.mode === "managed" || openrgb?.mode === "child";
  const legacy = openrgb?.legacy_task ?? false;
  const modeText = (() => {
    switch (openrgb?.mode) {
      case "managed":
        return legacy
          ? "Lumen runs OpenRGB through the scheduled task Lumen 0.2 set up."
          : "Lumen runs OpenRGB as its own Windows service, from a folder only administrators can change: started with Lumen, restarted if it stops answering, stopped when Lumen quits.";
      case "child":
        return "Lumen started this OpenRGB as its own process and stops it again when it quits.";
      case "service":
        return "Lumen is attached to the OpenRGB Windows service, which runs on its own and outlives Lumen.";
      case "external":
        return "Lumen is attached to an OpenRGB that was started elsewhere.";
      default:
        return "Lumen drives every device through OpenRGB's SDK server (port 6742): it attaches to a running OpenRGB, or starts the bundled copy silently in the background.";
    }
  })();

  return (
    <Section
      title="OpenRGB"
      aside={
        snapshot?.connected ? (
          <Badge tone="ok">connected</Badge>
        ) : snapshot?.problem ? (
          <Badge tone="danger">not responding</Badge>
        ) : openrgb?.server_reachable ? (
          <Badge tone="warn">server up, connecting…</Badge>
        ) : (
          <Badge tone="danger">not running</Badge>
        )
      }
    >
      {legacy && (
        <div className="flex flex-col gap-2 rounded-md border border-danger/40 bg-danger/10 p-2">
          <span className="flex items-center gap-1.5 text-xs font-medium text-danger">
            <ShieldAlert size={14} /> Security fix needed
          </span>
          <span className="text-xs text-muted">
            Lumen 0.2 ran OpenRGB through a scheduled task that any program on this PC could misuse to get administrator rights without asking. Switch to Lumen's OpenRGB
            service to close that hole: it runs a copy of OpenRGB that only administrators can change. One admin prompt.
          </span>
          <div>
            <Button size="sm" variant="primary" disabled={busy} onClick={() => adoptOpenrgb()}>
              <ShieldCheck size={14} /> {busy ? "Working…" : "Switch to the secure setup"}
            </Button>
          </div>
        </div>
      )}
      <p className="break-all font-mono text-[11px] text-faint">{openrgb?.exe ?? "OpenRGB executable not found"}</p>
      <p className="text-xs text-muted">{modeText}</p>
      <div className="flex flex-wrap gap-2">
        {owns ? (
          <Button size="sm" disabled={busy} onClick={() => restartOpenrgb()}>
            <RefreshCw size={14} className={busy ? "animate-spin" : undefined} /> Restart OpenRGB
          </Button>
        ) : (
          <Button size="sm" disabled={busy} onClick={() => startOpenrgb()}>
            <Power size={14} /> Start OpenRGB
          </Button>
        )}
        <Button size="sm" variant="ghost" onClick={() => refresh()}>
          <RefreshCw size={14} /> Rescan devices
        </Button>
      </div>
      {snapshot?.problem && (
        <div className="flex flex-col gap-2 rounded-md border border-danger/40 bg-danger/10 p-2">
          <span className="text-xs text-danger">{snapshot.problem}</span>
          {!owns && openrgb?.service && (
            <div>
              <Button size="sm" disabled={busy} onClick={() => restartOpenrgb()}>
                <RefreshCw size={14} /> Restart the OpenRGB service (admin prompt)
              </Button>
            </div>
          )}
        </div>
      )}
      {openrgb?.can_manage && openrgb.mode !== "managed" && (
        <div className="flex flex-col gap-2 rounded-md border border-border bg-surface-3 p-2">
          <span className="text-xs font-medium">Let Lumen manage OpenRGB</span>
          <span className="text-xs text-muted">
            Installs a small Windows service that runs Lumen's copy of OpenRGB from Program Files, with the administrator rights motherboard and RAM lighting need, and
            retires OpenRGB's own service if it is installed. Lumen then starts and stops OpenRGB together with itself. One admin prompt, once.
          </span>
          <div>
            <Button size="sm" variant="primary" disabled={busy} onClick={() => adoptOpenrgb()}>
              <ShieldCheck size={14} /> {busy ? "Working…" : "Let Lumen manage OpenRGB"}
            </Button>
          </div>
        </div>
      )}
      {openrgb?.mode === "managed" && (openrgb.stale || openrgb.outdated) && (
        <div className="flex flex-col gap-2 rounded-md border border-warn/40 bg-warn/10 p-2">
          <span className="text-xs text-warn">
            {openrgb.stale
              ? "Lumen's OpenRGB service runs a different copy of OpenRGB than this version ships (an earlier install, or a development build)."
              : "Lumen's OpenRGB service was set up by an older Lumen. Updating it lets OpenRGB run at a higher priority, so the lights stay smooth and in step while games or builds keep the PC busy. One admin prompt."}
          </span>
          <div>
            <Button size="sm" disabled={busy} onClick={() => adoptOpenrgb()}>
              <ShieldCheck size={14} /> {busy ? "Working…" : "Update Lumen's OpenRGB service"}
            </Button>
          </div>
        </div>
      )}
      {openrgb?.mode === "managed" && (
        <div className="flex items-center justify-between gap-3">
          <span className="text-xs text-faint">
            {legacy ? 'Task "LumenOpenRGB" in Task Scheduler.' : 'Service "LumenOpenRGB"; its files are in Program Files\\Lumen\\OpenRGB.'}
            {openrgb.service ? " OpenRGB's own service stays disabled." : ""}
          </span>
          <Button size="sm" variant="ghost" className="shrink-0 whitespace-nowrap" disabled={busy} onClick={() => releaseOpenrgb()}>
            {openrgb.service ? "Hand back to the service" : "Stop managing"}
          </Button>
        </div>
      )}
      {error && <p className="text-xs text-danger">{error}</p>}
    </Section>
  );
}

function PerformanceSection() {
  const setFps = useStore((s) => s.setFps);
  const fps = String(useStore((s) => s.snapshot?.fps ?? 60));
  return (
    <Section title="Performance">
      <Field label="Render rate" hint={`${fps} fps`}>
        <Select
          value={fps}
          options={[
            { value: "30", label: "30 fps — lightest" },
            { value: "60", label: "60 fps — smoothest" },
          ]}
          onChange={(v) => setFps(parseInt(v, 10))}
        />
      </Field>
      <ul className="flex flex-col gap-1 text-xs text-muted">
        <li>Static scenes are rendered once; the engine idles until something changes.</li>
        <li>Closing the window frees its memory entirely — lighting keeps running from the tray.</li>
        <li>Preview frames are only produced while the window is visible.</li>
      </ul>
    </Section>
  );
}

function UpdatesSection() {
  const app = useStore((s) => s.app);
  const update = useStore((s) => s.update);
  const checkForUpdate = useStore((s) => s.checkForUpdate);
  const installUpdate = useStore((s) => s.installUpdate);
  const setAutoUpdate = useStore((s) => s.setAutoUpdate);
  const busy = update.phase === "checking" || update.phase === "downloading" || update.phase === "installing";
  const mb = (n: number | undefined | null) => (n == null ? "…" : `${(n / 1048576).toFixed(1)} MB`);
  const pct = update.progress?.total ? Math.min(100, Math.round((update.progress.downloaded / update.progress.total) * 100)) : 0;
  return (
    <Section title="Updates" aside={<span className="font-mono text-[11px] text-faint">v{app?.version ?? "…"}</span>}>
      <ToggleRow
        title="Check for updates automatically"
        hint="Once a day, in the background. Nothing is installed until you press the button."
        checked={app?.auto_update ?? true}
        onChange={setAutoUpdate}
      />
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" disabled={busy} onClick={() => checkForUpdate()}>
          <RefreshCw size={14} className={update.phase === "checking" ? "animate-spin" : undefined} /> {update.phase === "checking" ? "Checking…" : "Check for updates"}
        </Button>
        {update.phase === "none" && <span className="text-xs text-ok">You're on the latest version.</span>}
        {update.phase === "error" && <span className="text-xs text-danger">{update.error}</span>}
      </div>
      {update.info && (
        <div className="flex flex-col gap-2 rounded-md border border-accent/40 bg-accent-soft/30 p-3">
          <div className="flex items-center justify-between">
            <span className="text-sm font-medium">Lumen {update.info.version} is available</span>
            {update.info.date && <span className="text-xs text-muted">{update.info.date}</span>}
          </div>
          {update.info.notes && <pre className="max-h-40 overflow-auto whitespace-pre-wrap font-sans text-xs text-muted">{update.info.notes}</pre>}
          {update.phase === "downloading" || update.phase === "installing" ? (
            <div className="flex flex-col gap-1">
              <div className="h-1.5 w-full overflow-hidden rounded-full bg-surface-3">
                <div className="h-full rounded-full bg-accent transition-[width] duration-300" style={{ width: `${update.phase === "installing" ? 100 : pct}%` }} />
              </div>
              <span className="text-xs text-muted">
                {update.phase === "installing"
                  ? "Installing… Lumen will restart by itself."
                  : `Downloading ${mb(update.progress?.downloaded)}${update.progress?.total ? ` of ${mb(update.progress.total)}` : ""}`}
              </span>
            </div>
          ) : (
            <div>
              <Button variant="primary" size="sm" onClick={() => installUpdate()}>
                <Download size={14} /> Install and restart
              </Button>
            </div>
          )}
        </div>
      )}
    </Section>
  );
}

function HelpSection() {
  const diagnostics = useStore((s) => s.diagnostics);
  const openLogFolder = useStore((s) => s.openLogFolder);
  const [state, setState] = useState<{ kind: "idle" | "copied" | "manual" | "error"; text?: string }>({ kind: "idle" });
  const copy = async () => {
    let text: string;
    try {
      text = await diagnostics();
    } catch (e) {
      setState({ kind: "error", text: e instanceof Error ? e.message : String(e) });
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      setState({ kind: "copied" });
    } catch {
      setState({ kind: "manual", text });
    }
  };
  const report = () => (isTauri ? openUrl(ISSUES_URL) : Promise.resolve(window.open(ISSUES_URL, "_blank"))).catch(() => window.open(ISSUES_URL, "_blank"));
  return (
    <Section title="Help">
      <p className="text-xs text-muted">
        Something not working? Copy the diagnostics (versions, OpenRGB setup, devices and recent logs, no serial numbers) and paste them into a bug report.
      </p>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" onClick={copy}>
          <ClipboardCopy size={14} /> {state.kind === "copied" ? "Copied" : "Copy diagnostics"}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => openLogFolder()}>
          <FolderOpen size={14} /> Open log folder
        </Button>
        <Button size="sm" variant="ghost" onClick={report}>
          <Bug size={14} /> Report a problem
        </Button>
      </div>
      {state.kind === "manual" && (
        <textarea readOnly value={state.text} className="h-32 w-full rounded-md border border-border bg-surface-3 p-2 font-mono text-[10px] text-muted" onFocus={(e) => e.currentTarget.select()} />
      )}
      {state.kind === "error" && <p className="text-xs text-danger">{state.text}</p>}
    </Section>
  );
}

export function SettingsPanel() {
  const app = useStore((s) => s.app);
  return (
    <Panel title="Settings">
      <StartupSection />
      <LightsSection />
      <AlertsSection />
      <OpenRgbSection />
      <PerformanceSection />
      <UpdatesSection />
      <HelpSection />
      <p className="text-[11px] text-faint">
        Lumen {app?.version ?? ""} · built on OpenRGB (GPL-2.0, bundled unmodified) · {isTauri ? "desktop" : "browser mock"}
      </p>
    </Panel>
  );
}
