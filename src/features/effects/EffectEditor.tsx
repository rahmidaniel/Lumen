import { Eraser, Plus, X } from "lucide-react";
import { Button, ColorInput, Field, IconButton, Select, Slider, TextInput, cx } from "../../components/ui";
import type { BlendMode, DeviceInfo, Effect, EffectType, Layer, Palette, Stop } from "../../lib/types";
import { useStore } from "../../store";

const NO_DEVICES: DeviceInfo[] = [];

export const effectLabels: Record<EffectType, string> = {
  static: "Static colour",
  gradient: "Gradient",
  spectrum: "Spectrum cycle",
  wave: "Wave",
  breathing: "Breathing",
  starlight: "Starlight",
  paint: "Paint keys",
};

export function defaultEffect(type: EffectType): Effect {
  switch (type) {
    case "static":
      return { type, color: { r: 0.55, g: 0.42, b: 1 } };
    case "gradient":
      return {
        type,
        angle: 0,
        stops: [
          { pos: 0, color: { r: 0.55, g: 0.42, b: 1 } },
          { pos: 1, color: { r: 0.13, g: 0.83, b: 0.93 } },
        ],
      };
    case "spectrum":
      return { type, speed: 0.1 };
    case "wave":
      return { type, angle: 0, speed: 0.25, wavelength: 1, palette: { type: "rainbow" } };
    case "breathing":
      return { type, color: { r: 0.55, g: 0.42, b: 1 }, speed: 0.25 };
    case "starlight":
      return { type, color: { r: 1, g: 1, b: 1 }, density: 0.15, speed: 0.6 };
    case "paint":
      return { type, strokes: [] };
  }
}

const blendOptions: { value: BlendMode; label: string }[] = [
  { value: "normal", label: "Normal" },
  { value: "add", label: "Add" },
  { value: "screen", label: "Screen" },
  { value: "multiply", label: "Multiply" },
];

const deg = (v: number) => `${Math.round(v)}°`;
const hz = (v: number) => `${v.toFixed(2)}/s`;
const pct = (v: number) => `${Math.round(v * 100)}%`;

function StopsEditor({ stops, onChange }: { stops: Stop[]; onChange: (s: Stop[]) => void }) {
  const update = (i: number, patch: Partial<Stop>) => onChange(stops.map((s, j) => (j === i ? { ...s, ...patch } : s)));
  return (
    <div className="flex flex-col gap-2">
      {stops.map((s, i) => (
        <div key={i} className="flex items-center gap-2">
          <ColorInput value={s.color} onChange={(color) => update(i, { color })} />
          <input
            type="range"
            min={0}
            max={1}
            step={0.01}
            value={s.pos}
            style={{ ["--fill" as string]: `${s.pos * 100}%` }}
            onChange={(e) => update(i, { pos: parseFloat(e.target.value) })}
          />
          <IconButton label="Remove stop" size="sm" disabled={stops.length <= 2} onClick={() => onChange(stops.filter((_, j) => j !== i))}>
            <X size={14} />
          </IconButton>
        </div>
      ))}
      <Button size="sm" variant="ghost" className="self-start" disabled={stops.length >= 6} onClick={() => onChange([...stops, { pos: 1, color: { r: 1, g: 1, b: 1 } }])}>
        <Plus size={14} /> Add stop
      </Button>
    </div>
  );
}

function PaletteEditor({ palette, onChange }: { palette: Palette; onChange: (p: Palette) => void }) {
  return (
    <div className="flex flex-col gap-2">
      <Field label="Palette">
        <Select<Palette["type"]>
          value={palette.type}
          options={[
            { value: "rainbow", label: "Rainbow" },
            { value: "stops", label: "Custom colours" },
          ]}
          onChange={(type) =>
            onChange(
              type === "rainbow"
                ? { type }
                : {
                    type,
                    stops: [
                      { pos: 0, color: { r: 0.55, g: 0.42, b: 1 } },
                      { pos: 0.5, color: { r: 0.13, g: 0.83, b: 0.93 } },
                    ],
                  },
            )
          }
        />
      </Field>
      {palette.type === "stops" && <StopsEditor stops={palette.stops} onChange={(stops) => onChange({ type: "stops", stops })} />}
    </div>
  );
}

/** Brush for a paint layer: the colour lives in the store so the canvas can use it. */
function PaintParams({ effect, onChange }: { effect: Extract<Effect, { type: "paint" }>; onChange: (e: Effect) => void }) {
  const color = useStore((s) => s.paintColor);
  const setColor = useStore((s) => s.setPaintColor);
  const painted = effect.strokes.reduce((n, s) => n + s.leds.length, 0);
  return (
    <>
      <Field label="Brush colour">
        <ColorInput value={color} onChange={setColor} />
      </Field>
      <p className="text-xs text-muted">
        With this layer selected, click or drag across keys and LEDs on the canvas to paint them. Hold Shift to erase. Keys you leave alone keep showing the layers below.
      </p>
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs text-faint">{painted === 0 ? "Nothing painted yet." : `${painted} LED${painted === 1 ? "" : "s"} painted`}</span>
        <Button size="sm" variant="ghost" disabled={painted === 0} onClick={() => onChange({ type: "paint", strokes: [] })}>
          <Eraser size={14} /> Clear all
        </Button>
      </div>
    </>
  );
}

function EffectParams({ effect, onChange }: { effect: Effect; onChange: (e: Effect) => void }) {
  switch (effect.type) {
    case "paint":
      return <PaintParams effect={effect} onChange={onChange} />;
    case "static":
      return (
        <Field label="Colour">
          <ColorInput value={effect.color} onChange={(color) => onChange({ ...effect, color })} />
        </Field>
      );
    case "gradient":
      return (
        <>
          <Slider label="Angle" value={effect.angle} min={0} max={360} step={1} format={deg} onChange={(angle) => onChange({ ...effect, angle })} />
          <StopsEditor stops={effect.stops} onChange={(stops) => onChange({ ...effect, stops })} />
        </>
      );
    case "spectrum":
      return <Slider label="Speed" value={effect.speed} min={0.01} max={1} format={hz} onChange={(speed) => onChange({ ...effect, speed })} />;
    case "wave":
      return (
        <>
          <Slider label="Speed" value={effect.speed} min={-2} max={2} format={hz} onChange={(speed) => onChange({ ...effect, speed })} />
          <Slider label="Wavelength" value={effect.wavelength} min={0.1} max={3} format={(v) => `${v.toFixed(2)}×`} onChange={(wavelength) => onChange({ ...effect, wavelength })} />
          <Slider label="Angle" value={effect.angle} min={0} max={360} step={1} format={deg} onChange={(angle) => onChange({ ...effect, angle })} />
          <PaletteEditor palette={effect.palette} onChange={(palette) => onChange({ ...effect, palette })} />
        </>
      );
    case "breathing":
      return (
        <>
          <Field label="Colour">
            <ColorInput value={effect.color} onChange={(color) => onChange({ ...effect, color })} />
          </Field>
          <Slider label="Speed" value={effect.speed} min={0.05} max={2} format={hz} onChange={(speed) => onChange({ ...effect, speed })} />
        </>
      );
    case "starlight":
      return (
        <>
          <Field label="Colour">
            <ColorInput value={effect.color} onChange={(color) => onChange({ ...effect, color })} />
          </Field>
          <Slider label="Density" value={effect.density} min={0} max={1} format={pct} onChange={(density) => onChange({ ...effect, density })} />
          <Slider label="Speed" value={effect.speed} min={0.1} max={3} format={hz} onChange={(speed) => onChange({ ...effect, speed })} />
        </>
      );
  }
}

export function EffectEditor({ layer }: { layer: Layer }) {
  const patch = useStore((s) => s.patchLayer);
  const devices = useStore((s) => s.snapshot?.devices) ?? NO_DEVICES;
  const set = (p: Partial<Layer>) => patch(layer.id, p);

  const maskDevices = layer.mask.type === "devices" ? layer.mask.ids : null;
  const toggleDevice = (id: number) => {
    const ids = maskDevices ?? [];
    const next = ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id];
    set({ mask: next.length === 0 ? { type: "all" } : { type: "devices", ids: next } });
  };

  return (
    <div className="flex flex-col gap-3 rounded-lg border border-border bg-surface-2 p-3">
      <div className="grid grid-cols-[1fr_auto] gap-2">
        <TextInput value={layer.name} onChange={(e) => set({ name: e.target.value })} aria-label="Layer name" />
      </div>
      <Field label="Effect">
        <Select<EffectType>
          value={layer.effect.type}
          options={(Object.keys(effectLabels) as EffectType[]).map((t) => ({ value: t, label: effectLabels[t] }))}
          onChange={(type) => set({ effect: defaultEffect(type), name: layer.name === effectLabels[layer.effect.type] ? effectLabels[type] : layer.name })}
        />
      </Field>
      <EffectParams effect={layer.effect} onChange={(effect) => set({ effect })} />

      <div className="my-1 h-px bg-border" />

      <Slider label="Opacity" value={layer.opacity} min={0} max={1} format={pct} onChange={(opacity) => set({ opacity })} />
      <Field label="Blend">
        <Select<BlendMode> value={layer.blend} options={blendOptions} onChange={(blend) => set({ blend })} />
      </Field>
      <Field label="Applies to" hint={maskDevices ? `${maskDevices.length} selected` : "all devices"}>
        <div className="flex flex-wrap gap-1.5">
          {devices.map((d) => {
            const on = maskDevices ? maskDevices.includes(d.id) : true;
            return (
              <button
                key={d.id}
                onClick={() => toggleDevice(d.id)}
                className={cx(
                  "rounded-full border px-2.5 py-1 text-xs transition-colors",
                  on ? "border-accent/50 bg-accent-soft text-text" : "border-border text-faint hover:text-muted",
                )}
              >
                {d.display_name}
              </button>
            );
          })}
        </div>
      </Field>
    </div>
  );
}
