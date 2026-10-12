import { Minus, Plus, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Field, IconButton, Select, cx } from "../../components/ui";
import type { DeviceInfo, DeviceSetting, DeviceSettings, SettingChange } from "../../lib/types";
import { useStore } from "../../store";

type DpiSetting = Extract<DeviceSetting, { type: "dpi_stages" }>;

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

function DpiStagesEditor({ setting, busy, onApply }: { setting: DpiSetting; busy: boolean; onApply: (change: SettingChange) => void }) {
  const [stages, setStages] = useState(setting.stages);
  const [active, setActive] = useState(setting.active);
  const key = `${setting.stages.join(",")}|${setting.active}`;
  useEffect(() => {
    setStages(setting.stages);
    setActive(setting.active);
    // Reset the draft whenever the device reports something new.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  const single = setting.max_stages === 1;
  const dirty = stages.join(",") !== setting.stages.join(",") || active !== setting.active;
  const invalid = stages.find((d) => !Number.isFinite(d) || d < setting.min || d > setting.max || d % setting.step !== 0);
  const update = (i: number, value: number) => setStages(stages.map((d, j) => (j === i ? value : d)));
  const remove = (i: number) => {
    const next = stages.filter((_, j) => j !== i);
    setStages(next);
    setActive(Math.min(active > i ? active - 1 : active, next.length - 1));
  };
  const add = () => {
    const last = stages.at(-1) ?? 800;
    setStages([...stages, Math.min(setting.max, Math.round((last * 2) / setting.step) * setting.step)]);
  };

  return (
    <div className="flex flex-col gap-2">
      <span className="text-xs text-muted">{setting.label}</span>
      <ul className="flex flex-col gap-1.5">
        {stages.map((dpi, i) => (
          <li key={i} className={cx("flex items-center gap-2 rounded-md border px-2 py-1", i === active && !single ? "border-accent/60 bg-accent-soft/30" : "border-border bg-surface-3")}>
            {!single && (
              <button
                className={cx("h-3.5 w-3.5 shrink-0 rounded-full border-2", i === active ? "border-accent bg-accent" : "border-border-strong")}
                aria-label={`Use stage ${i + 1}`}
                title="Use this stage"
                onClick={() => setActive(i)}
              />
            )}
            <span className="w-14 shrink-0 text-xs text-muted">{single ? "DPI" : `Stage ${i + 1}`}</span>
            <input
              type="number"
              min={setting.min}
              max={setting.max}
              step={setting.step}
              value={Number.isFinite(dpi) ? dpi : ""}
              onChange={(e) => update(i, parseInt(e.target.value, 10))}
              className="h-7 w-full min-w-0 rounded border border-border bg-surface-2 px-2 font-mono text-xs tabular-nums text-text outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
            />
            {!single && stages.length > 1 && (
              <IconButton label={`Remove stage ${i + 1}`} size="sm" onClick={() => remove(i)}>
                <Minus size={13} />
              </IconButton>
            )}
          </li>
        ))}
      </ul>
      <div className="flex flex-wrap items-center gap-2">
        {!single && stages.length < setting.max_stages && (
          <Button size="sm" variant="ghost" onClick={add}>
            <Plus size={13} /> Add stage
          </Button>
        )}
        {dirty && (
          <>
            <Button size="sm" variant="primary" disabled={busy || invalid !== undefined} onClick={() => onApply({ type: "dpi_stages", stages, active })}>
              {busy ? "Applying…" : "Apply"}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setStages(setting.stages);
                setActive(setting.active);
              }}
            >
              Reset
            </Button>
          </>
        )}
      </div>
      {invalid !== undefined && (
        <span className="text-xs text-danger">
          Use multiples of {setting.step} from {setting.min} to {setting.max} DPI.
        </span>
      )}
      <span className="text-xs text-faint">{setting.hint}</span>
    </div>
  );
}

/** The device's own settings, read from it when the panel opens. */
export function DeviceSettingsSection({ device }: { device: DeviceInfo }) {
  const read = useStore((s) => s.deviceSettings);
  const write = useStore((s) => s.setDeviceSetting);
  const [data, setData] = useState<DeviceSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  // Only the latest request may update the panel (selection can change mid-read).
  const latest = useRef(0);

  const load = useCallback(async () => {
    const ticket = ++latest.current;
    setLoading(true);
    setError(null);
    try {
      const settings = await read(device.id);
      if (ticket === latest.current) setData(settings);
    } catch (e) {
      if (ticket === latest.current) setError(message(e));
    } finally {
      if (ticket === latest.current) setLoading(false);
    }
  }, [device.id, read]);
  useEffect(() => {
    setData(null);
    load();
  }, [load]);

  const apply = async (id: string, change: SettingChange) => {
    const ticket = ++latest.current;
    setBusy(id);
    setError(null);
    try {
      const settings = await write(device.id, change);
      if (ticket === latest.current) setData(settings);
    } catch (e) {
      if (ticket === latest.current) setError(message(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <span className="text-xs font-semibold uppercase tracking-wide text-muted">Device settings</span>
        <IconButton label="Read the settings again" size="sm" disabled={loading} onClick={() => load()}>
          <RefreshCw size={13} className={loading ? "animate-spin" : undefined} />
        </IconButton>
      </div>
      {loading && !data && <p className="text-xs text-muted">Reading the settings from the device…</p>}
      {data && data.settings.length === 0 && <p className="text-xs text-muted">This device has no settings Lumen can change.</p>}
      {data?.settings.map((s) =>
        s.type === "choice" ? (
          <div key={s.id} className="flex flex-col gap-1">
            <Field label={s.label} hint={busy === s.id ? "applying…" : undefined}>
              <Select
                value={String(s.value)}
                options={s.options.map((o) => ({ value: String(o.value), label: o.label }))}
                onChange={(v) => apply(s.id, { type: "choice", id: s.id, value: parseInt(v, 10) })}
              />
            </Field>
            <span className="text-xs text-faint">{s.hint}</span>
          </div>
        ) : (
          <DpiStagesEditor key={s.id} setting={s} busy={busy === s.id} onApply={(change) => apply(s.id, change)} />
        ),
      )}
      {error && <p className="text-xs text-danger">{error}</p>}
      {data && data.settings.length > 0 && (
        <p className="text-xs text-faint">
          {data.persistence === "lumen"
            ? "This mouse keeps these only until it is switched off; Lumen puts them back whenever it reconnects."
            : "Stored on the device, so they stay without Lumen."}
        </p>
      )}
    </div>
  );
}
