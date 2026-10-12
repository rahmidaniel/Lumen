import { Power, RefreshCw, Settings } from "lucide-react";
import { Button } from "../../components/ui";
import type { Blackout } from "../../lib/types";
import { useStore } from "../../store";

const blackoutText: Record<Blackout, string> = {
  user: "Lights off",
  sleep: "Lights off while the PC sleeps",
  display: "Lights off while the screen is off",
  lock: "Lights off while the PC is locked",
  idle: "Lights off while the PC is idle — touch the mouse or keyboard to bring them back",
};

/** A chip over the canvas while every LED is dark. */
export function LightsOffNotice({ reason }: { reason: Blackout }) {
  const setLightsOff = useStore((s) => s.setLightsOff);
  return (
    <div className="pointer-events-auto absolute left-1/2 top-3 flex -translate-x-1/2 items-center gap-3 rounded-lg border border-warn/40 bg-surface/90 px-3 py-1.5 text-xs shadow-pop backdrop-blur">
      <Power size={13} className="text-warn" />
      <span className="text-text">{blackoutText[reason]}</span>
      {reason === "user" && (
        <Button size="sm" variant="ghost" onClick={() => setLightsOff(false)}>
          Turn on
        </Button>
      )}
    </div>
  );
}

/** Shown when OpenRGB is up but reports nothing: what usually causes that. */
export function NoDevicesHelp() {
  const refresh = useStore((s) => s.refreshDevices);
  const setTab = useStore((s) => s.setTab);
  const conflicts = useStore((s) => s.conflicts);
  const windows = navigator.platform.startsWith("Win");
  const mac = navigator.platform.startsWith("Mac");
  return (
    <div className="pointer-events-auto flex max-w-md flex-col gap-3 rounded-xl border border-border bg-surface/95 p-5 text-left shadow-pop backdrop-blur">
      <div className="text-sm font-semibold">OpenRGB is running but found no devices</div>
      <ul className="flex list-disc flex-col gap-1.5 pl-4 text-xs text-muted">
        {conflicts.length > 0 && (
          <li>
            <span className="text-warn">{conflicts.map((c) => c.name).join(", ")}</span> {conflicts.length === 1 ? "is" : "are"} running and may be holding your devices. Quit
            {conflicts.length === 1 ? " it" : " them"}, then rescan.
          </li>
        )}
        <li>
          Check that OpenRGB supports your hardware on its{" "}
          <a className="text-accent underline-offset-2 hover:underline" href="https://openrgb.org/devices.html" target="_blank" rel="noreferrer">
            device list
          </a>
          .
        </li>
        <li>Quit vendor apps (Synapse, G HUB, iCUE, …): some keep their devices to themselves while they run.</li>
        {windows && <li>Motherboards and RAM need administrator rights: Settings → OpenRGB → Let Lumen manage OpenRGB.</li>}
        {mac && <li>Keyboards need the Input Monitoring permission for OpenRGB (System Settings → Privacy &amp; Security).</li>}
        {!windows && !mac && <li>Without OpenRGB's udev rules, devices are only reachable as root; see the install notes.</li>}
        <li>Plugged something in just now? Give it a few seconds, then rescan.</li>
      </ul>
      <div className="flex gap-2">
        <Button size="sm" onClick={() => refresh()}>
          <RefreshCw size={14} /> Rescan devices
        </Button>
        <Button size="sm" variant="ghost" onClick={() => setTab("settings")}>
          <Settings size={14} /> OpenRGB settings
        </Button>
      </div>
    </div>
  );
}
