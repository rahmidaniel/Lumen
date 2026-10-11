import { Power, RefreshCw } from "lucide-react";
import { Badge, Button, Panel } from "../../components/ui";
import { isTauri } from "../../lib/ipc";
import { useStore } from "../../store";

export function SettingsPanel() {
  const openrgb = useStore((s) => s.openrgb);
  const snapshot = useStore((s) => s.snapshot);
  const error = useStore((s) => s.error);
  const startOpenrgb = useStore((s) => s.startOpenrgb);
  const refresh = useStore((s) => s.refreshDevices);

  return (
    <Panel title="Settings">
      <div className="flex flex-col gap-2 rounded-lg border border-border bg-surface-2 p-3">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium">OpenRGB</span>
          {snapshot?.connected ? <Badge tone="ok">connected</Badge> : openrgb?.server_reachable ? <Badge tone="warn">server up, connecting…</Badge> : <Badge tone="danger">not running</Badge>}
        </div>
        <p className="break-all font-mono text-[11px] text-faint">{openrgb?.exe ?? "OpenRGB executable not found"}</p>
        <p className="text-xs text-muted">
          Lumen drives every device through OpenRGB's SDK server (port 6742). It starts OpenRGB headless on launch; if you prefer, run OpenRGB yourself with <code className="font-mono">--server</code>.
        </p>
        <div className="flex gap-2">
          <Button size="sm" onClick={() => startOpenrgb()}>
            <Power size={14} /> Start OpenRGB
          </Button>
          <Button size="sm" variant="ghost" onClick={() => refresh()}>
            <RefreshCw size={14} /> Rescan devices
          </Button>
        </div>
        {error && <p className="text-xs text-danger">{error}</p>}
      </div>

      <div className="flex flex-col gap-1 rounded-lg border border-border bg-surface-2 p-3 text-xs text-muted">
        <div className="flex justify-between">
          <span>Render rate</span>
          <span className="font-mono text-text">60 fps</span>
        </div>
        <div className="flex justify-between">
          <span>Preview rate</span>
          <span className="font-mono text-text">30 fps</span>
        </div>
        <div className="flex justify-between">
          <span>Mode</span>
          <span className="font-mono text-text">{isTauri ? "desktop" : "browser mock"}</span>
        </div>
      </div>

      <p className="text-[11px] text-faint">Lumen 0.1 · built on OpenRGB. Close the window to keep lighting running from the tray.</p>
    </Panel>
  );
}
