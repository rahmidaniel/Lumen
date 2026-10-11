import { Pause, Play } from "lucide-react";
import { useStore } from "../store";
import { Badge, IconButton, cx } from "./ui";

export function TopBar() {
  const snapshot = useStore((s) => s.snapshot);
  const openrgb = useStore((s) => s.openrgb);
  const setBrightness = useStore((s) => s.setBrightness);
  const setPaused = useStore((s) => s.setPaused);

  const connected = snapshot?.connected ?? false;
  const paused = snapshot?.paused ?? false;
  const brightness = snapshot?.scene.brightness ?? 1;

  let status: { tone: "ok" | "warn" | "danger"; text: string };
  if (connected) status = { tone: "ok", text: `OpenRGB connected · ${snapshot?.devices.length ?? 0} devices` };
  else if (openrgb && !openrgb.exe) status = { tone: "danger", text: "OpenRGB not installed" };
  else status = { tone: "warn", text: "Connecting to OpenRGB…" };

  return (
    <header className="flex items-center justify-between px-4 border-b border-border bg-surface/80 backdrop-blur">
      <div className="flex items-center gap-3">
        <span className="h-6 w-6 rounded-lg bg-[conic-gradient(from_200deg,#8b6cff,#22d3ee,#34d399,#fbbf24,#f87171,#8b6cff)] shadow-[0_0_24px_-4px_#8b6cff]" />
        <span className="text-[15px] font-semibold tracking-tight">Lumen</span>
      </div>

      <div className="flex items-center gap-4">
        <Badge tone={status.tone}>
          <span className={cx("h-1.5 w-1.5 rounded-full", status.tone === "ok" ? "bg-ok" : status.tone === "warn" ? "bg-warn animate-pulse" : "bg-danger")} />
          {status.text}
        </Badge>

        <label className="flex items-center gap-2 text-xs text-muted">
          Brightness
          <input
            type="range"
            min={0}
            max={1}
            step={0.01}
            value={brightness}
            style={{ ["--fill" as string]: `${brightness * 100}%`, width: 120 }}
            onChange={(e) => setBrightness(parseFloat(e.target.value))}
          />
          <span className="w-9 text-right font-mono tabular-nums">{Math.round(brightness * 100)}%</span>
        </label>

        <IconButton label={paused ? "Resume lighting" : "Pause lighting"} active={paused} onClick={() => setPaused(!paused)}>
          {paused ? <Play size={16} /> : <Pause size={16} />}
        </IconButton>
      </div>
    </header>
  );
}
