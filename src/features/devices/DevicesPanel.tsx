import { BatteryCharging, BatteryFull, BatteryLow, BatteryMedium, Cpu, Fan, Gamepad2, Headphones, HardDriveDownload, Keyboard, Lightbulb, MemoryStick, Monitor, Mouse, RefreshCw, Square } from "lucide-react";
import { useState } from "react";
import { Badge, Button, ColorInput, Field, IconButton, NumberInput, Panel, Select, cx } from "../../components/ui";
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

function BatteryBadge({ battery }: { battery: Battery }) {
  const Icon = battery.charging ? BatteryCharging : battery.percent > 60 ? BatteryFull : battery.percent > 25 ? BatteryMedium : BatteryLow;
  const tone = battery.charging ? "ok" : battery.percent <= 15 ? "danger" : battery.percent <= 30 ? "warn" : "neutral";
  return (
    <Badge tone={tone}>
      <Icon size={12} />
      {battery.percent}%{battery.charging ? " · charging" : ""}
    </Badge>
  );
}

function DeviceCard({ device, active, onSelect }: { device: DeviceInfo; active: boolean; onSelect: () => void }) {
  const Icon = icons[device.kind_label] ?? Cpu;
  return (
    <button
      onClick={onSelect}
      className={cx(
        "w-full rounded-lg border p-3 text-left transition-colors",
        active ? "border-accent/60 bg-accent-soft/40" : "border-border bg-surface-2 hover:bg-surface-3",
      )}
    >
      <div className="flex items-start gap-3">
        <span className={cx("mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-md", active ? "bg-accent/20 text-[#c4b5fd]" : "bg-surface-3 text-muted")}>
          <Icon size={16} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium">{device.display_name}</div>
          <div className="truncate text-xs text-muted">
            {device.vendor || device.kind_label} · {device.led_count} LEDs
          </div>
          <div className="mt-2 flex flex-wrap gap-1">
            {device.streamable ? <Badge tone="ok">live</Badge> : <Badge tone="warn">no per-LED mode</Badge>}
            {device.onboard && <Badge tone="accent">onboard memory</Badge>}
            {device.battery && <BatteryBadge battery={device.battery} />}
          </div>
        </div>
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
  const setPlacement = useStore((s) => s.setPlacement);
  const refresh = useStore((s) => s.refreshDevices);

  const devices = snapshot?.devices ?? [];
  const device = devices.find((d) => d.id === selected) ?? null;
  const placement = device ? snapshot?.scene.layout.placements[String(device.id)] : undefined;

  return (
    <Panel
      title="Devices"
      actions={
        <IconButton label="Rescan devices" size="sm" onClick={() => refresh()}>
          <RefreshCw size={14} />
        </IconButton>
      }
    >
      {devices.length === 0 && <p className="text-sm text-muted">No devices yet. OpenRGB reports them here once it is running.</p>}
      <div className="flex flex-col gap-2">
        {devices.map((d) => (
          <DeviceCard key={d.id} device={d} active={d.id === selected} onSelect={() => selectDevice(d.id === selected ? null : d.id)} />
        ))}
      </div>

      {device && placement && (
        <div className="mt-2 flex flex-col gap-4 rounded-lg border border-border bg-surface-2 p-3">
          <div className="flex flex-col gap-3">
            <div className="text-xs font-semibold uppercase tracking-wide text-muted">Placement (mm)</div>
            <div className="grid grid-cols-2 gap-2">
              {(["x", "y", "width", "height"] as const).map((k) => (
                <Field key={k} label={k}>
                  <NumberInput value={placement[k]} onChange={(v) => setPlacement(device.id, { ...placement, [k]: v }, true)} />
                </Field>
              ))}
              <Field label="rotation °">
                <NumberInput value={placement.rotation} onChange={(v) => setPlacement(device.id, { ...placement, rotation: v }, true)} />
              </Field>
            </div>
          </div>

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
          </div>

          {device.onboard === "razer" && <BootLighting device={device} />}
          {device.onboard === "openrgb" && (
            <p className="text-xs text-muted">
              OpenRGB can store these modes on this device: <span className="text-text">{device.saveable_modes.join(", ")}</span>. Saving them from Lumen is coming next.
            </p>
          )}

          <p className="truncate font-mono text-[11px] text-faint" title={device.location}>
            {device.location}
          </p>
        </div>
      )}
    </Panel>
  );
}
