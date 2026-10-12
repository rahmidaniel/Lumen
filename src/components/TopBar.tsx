import { BatteryCharging, BatteryFull, BatteryLow, BatteryMedium, Cpu, Keyboard, Mouse, Pause, Play, Power, ShieldAlert, TriangleAlert } from "lucide-react";
import { useStore } from "../store";
import { Badge, IconButton, cx } from "./ui";
import type { Battery, DeviceInfo } from "../lib/types";

const kindIcons: Record<string, typeof Cpu> = { keyboard: Keyboard, keypad: Keyboard, mouse: Mouse };

function batteryIcon(b: Battery) {
  if (b.charging) return BatteryCharging;
  if (b.percent > 60) return BatteryFull;
  if (b.percent > 25) return BatteryMedium;
  return BatteryLow;
}

/** Compact battery chip: device-kind icon + percentage, full name on hover. */
function BatteryChip({ device, battery }: { device: DeviceInfo; battery: Battery }) {
  const Kind = kindIcons[device.kind_label] ?? Cpu;
  const Level = batteryIcon(battery);
  const tone = battery.charging ? "ok" : battery.percent <= 15 ? "danger" : battery.percent <= 30 ? "warn" : "neutral";
  const v = battery.millivolts ? ` · ${(battery.millivolts / 1000).toFixed(2)} V` : "";
  return (
    <span title={`${device.display_name}: ${battery.percent}%${battery.charging ? ", charging" : ""}${v}`}>
      <Badge tone={tone} className="h-7 gap-1.5 px-2.5 text-xs">
        <Kind size={13} />
        <span className="font-semibold tabular-nums text-text">{battery.percent}%</span>
        <Level size={13} className={cx(battery.charging && "text-ok")} />
      </Badge>
    </span>
  );
}

export function TopBar() {
  const snapshot = useStore((s) => s.snapshot);
  const openrgb = useStore((s) => s.openrgb);
  const conflicts = useStore((s) => s.conflicts);
  const setTab = useStore((s) => s.setTab);
  const setBrightness = useStore((s) => s.setBrightness);
  const setPaused = useStore((s) => s.setPaused);
  const setLightsOff = useStore((s) => s.setLightsOff);

  const update = useStore((s) => s.update.info);
  const connected = snapshot?.connected ?? false;
  const paused = snapshot?.paused ?? false;
  const lightsOff = snapshot?.blackout === "user";
  const brightness = snapshot?.scene.brightness ?? 1;
  const powered = (snapshot?.devices ?? []).filter((d) => d.battery);

  let status: { tone: "ok" | "warn" | "danger"; text: string };
  if (connected) status = { tone: "ok", text: `OpenRGB · ${snapshot?.devices.length ?? 0} devices` };
  else if (snapshot?.problem) status = { tone: "danger", text: "OpenRGB not responding" };
  else if (openrgb && !openrgb.exe) status = { tone: "danger", text: "OpenRGB not installed" };
  else status = { tone: "warn", text: "Connecting to OpenRGB…" };

  return (
    <header className="flex items-center justify-between border-b border-border bg-surface/80 px-4 backdrop-blur">
      <div className="flex items-center gap-3">
        <span className="h-6 w-6 rounded-lg bg-[conic-gradient(from_200deg,#8b6cff,#22d3ee,#34d399,#fbbf24,#f87171,#8b6cff)] shadow-[0_0_24px_-4px_#8b6cff]" />
        <span className="text-[15px] font-semibold tracking-tight">Lumen</span>
        {powered.length > 0 && (
          <button className="ml-2 flex items-center gap-1.5" onClick={() => setTab("devices")} title="Open devices">
            {powered.map((d) => (
              <BatteryChip key={d.id} device={d} battery={d.battery!} />
            ))}
          </button>
        )}
      </div>

      <div className="flex items-center gap-4">
        {conflicts.length > 0 && (
          <button onClick={() => setTab("devices")} title="Another lighting app is running — open Devices for details">
            <Badge tone="warn">
              <TriangleAlert size={12} /> {conflicts.length === 1 ? `${conflicts[0].name} running` : `${conflicts.length} lighting apps running`}
            </Badge>
          </button>
        )}
        {openrgb?.legacy_task && (
          <button onClick={() => setTab("settings")} title="Lumen 0.2's OpenRGB task can be misused by other programs — open Settings to fix it">
            <Badge tone="danger">
              <ShieldAlert size={12} /> Security fix needed
            </Badge>
          </button>
        )}
        {update && (
          <button onClick={() => setTab("settings")} title={`Lumen ${update.version} is available — open Settings to install`}>
            <Badge tone="accent">Update {update.version}</Badge>
          </button>
        )}
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

        <div className="flex items-center gap-1">
          <IconButton label={paused ? "Resume effects" : "Pause effects"} active={paused} onClick={() => setPaused(!paused)}>
            {paused ? <Play size={16} /> : <Pause size={16} />}
          </IconButton>
          <IconButton
            label={lightsOff ? "Turn the lights on" : "Turn the lights off"}
            active={lightsOff}
            className={cx(lightsOff && "text-warn")}
            onClick={() => setLightsOff(!lightsOff)}
          >
            <Power size={16} />
          </IconButton>
        </div>
      </div>
    </header>
  );
}
