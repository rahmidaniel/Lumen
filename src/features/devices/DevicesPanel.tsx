import { BatteryCharging, Cpu, Fan, Gamepad2, Headphones, HardDriveDownload, Keyboard, Lightbulb, MemoryStick, Monitor, Mouse, RefreshCw, Square } from "lucide-react";
import { useState } from "react";
import { ConflictBanner } from "../../components/ConflictBanner";
import { Badge, Button, ColorInput, Field, IconButton, Panel, Select, cx } from "../../components/ui";
import type { Battery, DeviceInfo, OnboardEffect, Rgb } from "../../lib/types";
import { useStore } from "../../store";

const icons: Record<string, typeof Cpu> = {
  keyboard: Keyboard,
  keypad: Keyboard,
  mouse: Mouse,
  mousemat: Square,
  headset: Headphones,
  headset_stand: Headphones,
  gpu: Monitor,
  motherboard: Cpu,
  dram: MemoryStick,
  cooler: Fan,
  light: Lightbulb,
  ledstrip: Lightbulb,
  gamepad: Gamepad2,
};

export const batteryTone = (b: Battery): "ok" | "warn" | "danger" => (b.charging ? "ok" : b.percent <= 15 ? "danger" : b.percent <= 30 ? "warn" : "ok");

export const volts = (b: Battery) => (b.millivolts ? `${(b.millivolts / 1000).toFixed(2)} V` : null);

function BatteryGauge({ battery }: { battery: Battery }) {
  const tone = batteryTone(battery);
  const v = volts(battery);
  return (
    <div className="flex shrink-0 flex-col items-end gap-1.5" title={`${battery.charging ? "Charging" : "Battery"}${v ? ` · cell ${v}` : ""}`}>
      <div className="flex items-center gap-1.5">
        {battery.charging && <BatteryCharging size={16} className="text-ok" />}
        <span className="text-2xl font-semibold leading-none tabular-nums">
          {battery.percent}
          <span className="ml-0.5 text-sm font-medium text-muted">%</span>
        </span>
      </div>
      <div className="h-1.5 w-24 overflow-hidden rounded-full bg-surface-3">
        <div
          className={cx("h-full rounded-full transition-[width] duration-500", tone === "ok" ? "bg-ok" : tone === "warn" ? "bg-warn" : "bg-danger")}
          style={{ width: `${Math.max(3, battery.percent)}%` }}
        />
      </div>
      {v && <span className="font-mono text-[10px] tabular-nums text-faint">{v}</span>}
    </div>
  );
}

function DeviceCard({ device, active, onSelect }: { device: DeviceInfo; active: boolean; onSelect: () => void }) {
  const Icon = icons[device.kind_label] ?? Cpu;
  const body = useStore((s) => s.snapshot?.geometry.find((g) => g.device === device.id)?.body);
  const layout = body?.type === "keyboard" ? body.layout : null;
  const zones = device.zones.length;
  return (
    <button
      onClick={onSelect}
      className={cx(
        "w-full rounded-lg border p-3 text-left transition-colors",
        active ? "border-accent/60 bg-accent-soft/40" : "border-border bg-surface-2 hover:bg-surface-3",
      )}
    >
      <div className="flex items-start gap-3">
        <span className={cx("mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-md", active ? "bg-accent/20 text-[#c4b5fd]" : "bg-surface-3 text-muted")}>
          <Icon size={18} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium">{device.display_name}</div>
          <div className="truncate text-xs text-muted">
            {device.vendor || device.kind_label} · {device.led_count} LEDs · {zones} {zones === 1 ? "zone" : "zones"}
          </div>
          <div className="mt-2 flex flex-wrap gap-1">
            {device.streamable ? <Badge tone="ok">live</Badge> : <Badge tone="warn">no per-LED mode</Badge>}
            {device.onboard && <Badge tone="accent">onboard memory</Badge>}
            {layout && <Badge tone="neutral">{layout.toUpperCase()}</Badge>}
          </div>
        </div>
        {device.battery && <BatteryGauge battery={device.battery} />}
      </div>
    </button>
  );
}

type OnboardType = OnboardEffect["type"];
const onboardOptions: { value: OnboardType; label: string }[] = [
  { value: "static", label: "Static colour" },
  { value: "spectrum", label: "Spectrum cycle" },
  { value: "wave", label: "Wave" },
  { value: "breathing", label: "Breathing" },
  { value: "off", label: "Off" },
];

function BootLighting({ device }: { device: DeviceInfo }) {
  const saveToDevice = useStore((s) => s.saveToDevice);
  const [type, setType] = useState<OnboardType>("spectrum");
  const [color, setColor] = useState<Rgb>({ r: 0.55, g: 0.42, b: 1 });
  const [direction, setDirection] = useState<"1" | "2">("1");
  const [state, setState] = useState<{ kind: "idle" | "busy" | "done" | "error"; text?: string }>({ kind: "idle" });

  const effect = (): OnboardEffect => {
    switch (type) {
      case "static":
        return { type, color };
      case "breathing":
        return { type, color };
      case "wave":
        return { type, direction: direction === "1" ? 1 : 2 };
      default:
        return { type };
    }
  };

  const save = async () => {
    setState({ kind: "busy" });
    try {
      await saveToDevice(device.id, effect());
      setState({ kind: "done", text: "Stored. The device shows this until Lumen takes over after login." });
    } catch (e) {
      setState({ kind: "error", text: e instanceof Error ? e.message : String(e) });
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="text-xs font-semibold uppercase tracking-wide text-muted">Boot lighting</div>
      <p className="text-xs text-muted">What the device plays on its own — during boot and before Lumen starts. Written to the device's memory.</p>
      <Field label="Effect">
        <Select<OnboardType> value={type} options={onboardOptions} onChange={setType} />
      </Field>
      {(type === "static" || type === "breathing") && (
        <Field label="Colour">
          <ColorInput value={color} onChange={setColor} />
        </Field>
      )}
      {type === "wave" && (
        <Field label="Direction">
          <Select<"1" | "2">
            value={direction}
            options={[
              { value: "1", label: "Left → right" },
              { value: "2", label: "Right → left" },
            ]}
            onChange={setDirection}
          />
        </Field>
      )}
      <div className="flex items-center gap-2">
        <Button variant="primary" size="sm" disabled={state.kind === "busy"} onClick={save}>
          <HardDriveDownload size={14} /> {state.kind === "busy" ? "Saving…" : "Save to device"}
        </Button>
        {state.text && <span className={cx("text-xs", state.kind === "error" ? "text-danger" : "text-ok")}>{state.text}</span>}
      </div>
    </div>
  );
}

export function DevicesPanel() {
  const snapshot = useStore((s) => s.snapshot);
  const selected = useStore((s) => s.selectedDevice);
  const selectDevice = useStore((s) => s.selectDevice);
  const refresh = useStore((s) => s.refreshDevices);

  const devices = snapshot?.devices ?? [];
  const device = devices.find((d) => d.id === selected) ?? null;

  return (
    <Panel
      title="Devices"
      actions={
        <IconButton label="Rescan devices" size="sm" onClick={() => refresh()}>
          <RefreshCw size={14} />
        </IconButton>
      }
    >
      <ConflictBanner />
      {devices.length === 0 && (
        <p className="text-sm text-muted">
          {snapshot?.connected && !snapshot.detecting
            ? "OpenRGB is running but reports no devices. The canvas lists what usually causes that."
            : "No devices yet. OpenRGB reports them here once it is running."}
        </p>
      )}
      <div className="flex flex-col gap-2">
        {devices.map((d) => (
          <DeviceCard key={d.id} device={d} active={d.id === selected} onSelect={() => selectDevice(d.id === selected ? null : d.id)} />
        ))}
      </div>

      {device && (
        <div className="mt-2 flex flex-col gap-4 rounded-lg border border-border bg-surface-2 p-3">
          <div className="flex flex-col gap-2">
            <div className="text-xs font-semibold uppercase tracking-wide text-muted">Zones</div>
            <ul className="flex flex-col gap-1 text-xs">
              {device.zones.map((z, i) => (
                <li key={i} className="flex items-center justify-between rounded-md bg-surface-3 px-2 py-1.5">
                  <span className="truncate">{z.name}</span>
                  <span className="font-mono text-muted">{z.matrix ? `${z.matrix[0]}×${z.matrix[1]} grid` : `${z.led_count} LEDs`}</span>
                </li>
              ))}
            </ul>
            <p className="text-xs text-faint">Drag the device on the canvas to place it; scroll to zoom.</p>
          </div>

          {device.onboard === "razer" && <BootLighting device={device} />}
          {device.onboard === "openrgb" && (
            <p className="text-xs text-muted">
              OpenRGB can store these modes on this device: <span className="text-text">{device.saveable_modes.join(", ")}</span>. Saving them from Lumen is coming next.
            </p>
          )}
        </div>
      )}
    </Panel>
  );
}
