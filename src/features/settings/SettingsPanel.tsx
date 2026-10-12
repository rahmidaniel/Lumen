import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { Download, Power, RefreshCw, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge, Button, Field, Panel, Select, Toggle } from "../../components/ui";
import { isTauri } from "../../lib/ipc";
import { useStore } from "../../store";

export function SettingsPanel() {
  const openrgb = useStore((s) => s.openrgb);
  const snapshot = useStore((s) => s.snapshot);
  const error = useStore((s) => s.error);
  const startOpenrgb = useStore((s) => s.startOpenrgb);
  const restartOpenrgb = useStore((s) => s.restartOpenrgb);
  const adoptOpenrgb = useStore((s) => s.adoptOpenrgb);
  const releaseOpenrgb = useStore((s) => s.releaseOpenrgb);
  const openrgbBusy = useStore((s) => s.openrgbBusy);
  const ownsOpenrgb = openrgb?.mode === "managed" || openrgb?.mode === "child";
  const modeText = (() => {
    switch (openrgb?.mode) {
      case "managed":
        return "Lumen runs OpenRGB itself through an elevated task: started with Lumen, restarted if it stops answering, stopped when Lumen quits.";
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
  const refresh = useStore((s) => s.refreshDevices);
  const setFps = useStore((s) => s.setFps);
  const fps = String(snapshot?.fps ?? 60);
  const app = useStore((s) => s.app);
  const update = useStore((s) => s.update);
  const checkForUpdate = useStore((s) => s.checkForUpdate);
  const installUpdate = useStore((s) => s.installUpdate);
  const setAutoUpdate = useStore((s) => s.setAutoUpdate);
  const busy = update.phase === "checking" || update.phase === "downloading" || update.phase === "installing";
  const mb = (n: number | undefined | null) => (n == null ? "…" : `${(n / 1048576).toFixed(1)} MB`);
  const pct = update.progress?.total ? Math.min(100, Math.round((update.progress.downloaded / update.progress.total) * 100)) : 0;

  // Autostart lives in the OS (registry / LaunchAgent), not in our store.
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autostartError, setAutostartError] = useState<string | null>(null);
  useEffect(() => {
    if (!isTauri) return;
    isEnabled()
      .then(setAutostart)
      .catch(() => setAutostart(null));
  }, []);
  const toggleAutostart = async (on: boolean) => {
    try {
      if (on) await enable();
      else await disable();
      setAutostart(on);
      setAutostartError(null);
    } catch (e) {
      setAutostartError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <Panel title="Settings">
      <div className="flex flex-col gap-3 rounded-lg border border-border bg-surface-2 p-3">
        <span className="text-sm font-medium">Startup</span>
        <div className="flex items-center justify-between gap-3">
          <div className="flex flex-col">
            <span className="text-sm">{navigator.platform.startsWith("Mac") ? "Start with macOS" : navigator.platform.startsWith("Win") ? "Start with Windows" : "Start at login"}</span>
            <span className="text-xs text-muted">Launches hidden in the tray and lights your devices as soon as you log in.</span>
          </div>
          <Toggle checked={autostart === true} onChange={toggleAutostart} label="Start at login" />
        </div>
        {autostart === null && isTauri && <p className="text-xs text-faint">Autostart state unavailable.</p>}
        {autostartError && <p className="text-xs text-danger">{autostartError}</p>}
      </div>

      <div className="flex flex-col gap-2 rounded-lg border border-border bg-surface-2 p-3">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium">OpenRGB</span>
          {snapshot?.connected ? (
            <Badge tone="ok">connected</Badge>
          ) : snapshot?.problem ? (
            <Badge tone="danger">not responding</Badge>
          ) : openrgb?.server_reachable ? (
            <Badge tone="warn">server up, connecting…</Badge>
          ) : (
            <Badge tone="danger">not running</Badge>
          )}
        </div>
        <p className="break-all font-mono text-[11px] text-faint">{openrgb?.exe ?? "OpenRGB executable not found"}</p>
        <p className="text-xs text-muted">{modeText}</p>
        <div className="flex flex-wrap gap-2">
          {ownsOpenrgb ? (
            <Button size="sm" disabled={openrgbBusy} onClick={() => restartOpenrgb()}>
              <RefreshCw size={14} className={openrgbBusy ? "animate-spin" : undefined} /> Restart OpenRGB
            </Button>
          ) : (
            <Button size="sm" disabled={openrgbBusy} onClick={() => startOpenrgb()}>
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
            {!ownsOpenrgb && openrgb?.service && (
              <div>
                <Button size="sm" disabled={openrgbBusy} onClick={() => restartOpenrgb()}>
                  <RefreshCw size={14} /> Restart the OpenRGB service (admin prompt)
                </Button>
              </div>
            )}
          </div>
        )}
        {openrgb?.can_manage && openrgb.mode !== "managed" && (
          <div className="flex flex-col gap-2 rounded-md border border-border bg-surface-3 p-2">
            <span className="text-xs font-medium">Let Lumen run OpenRGB</span>
            <span className="text-xs text-muted">
              Stops and disables the OpenRGB Windows service and registers a task that runs Lumen's bundled OpenRGB with administrator rights, so graphics cards and motherboards keep working. From then on Lumen starts OpenRGB when it starts, restarts it if it stops answering, and stops it when you quit. One admin prompt, once.
            </span>
            <div>
              <Button size="sm" variant="primary" disabled={openrgbBusy} onClick={() => adoptOpenrgb()}>
                <ShieldCheck size={14} /> {openrgbBusy ? "Working…" : "Let Lumen manage OpenRGB"}
              </Button>
            </div>
          </div>
        )}
        {openrgb?.mode === "managed" && (
          <div className="flex items-center justify-between gap-3">
            <span className="text-xs text-faint">Task "LumenOpenRGB" in Task Scheduler; the OpenRGB Windows service stays disabled.</span>
            <Button size="sm" variant="ghost" className="shrink-0 whitespace-nowrap" disabled={openrgbBusy} onClick={() => releaseOpenrgb()}>
              Hand back to the service
            </Button>
          </div>
        )}
        {error && <p className="text-xs text-danger">{error}</p>}
      </div>

      <div className="flex flex-col gap-3 rounded-lg border border-border bg-surface-2 p-3">
        <span className="text-sm font-medium">Performance</span>
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
      </div>

      <div className="flex flex-col gap-3 rounded-lg border border-border bg-surface-2 p-3">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium">Updates</span>
          <span className="font-mono text-[11px] text-faint">v{app?.version ?? "…"}</span>
        </div>
        <div className="flex items-center justify-between gap-3">
          <div className="flex flex-col">
            <span className="text-sm">Check for updates automatically</span>
            <span className="text-xs text-muted">Once a day, in the background. Nothing is installed until you press the button.</span>
          </div>
          <Toggle checked={app?.auto_update ?? true} onChange={setAutoUpdate} label="Check for updates automatically" />
        </div>
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
      </div>

      <p className="text-[11px] text-faint">
        Lumen {app?.version ?? ""} · built on OpenRGB (GPL-2.0, bundled unmodified) · {isTauri ? "desktop" : "browser mock"}
      </p>
    </Panel>
  );
}
