import { TriangleAlert, X } from "lucide-react";
import type { Conflict } from "../lib/types";
import { useStore } from "../store";
import { IconButton } from "./ui";

/** Vendor lighting apps running alongside Lumen, with how to get them out of the way. */
export function ConflictBanner() {
  const conflicts = useStore((s) => s.conflicts);
  const dismiss = useStore((s) => s.dismissConflict);
  if (conflicts.length === 0) return null;
  return (
    <div className="flex flex-col gap-2">
      {conflicts.map((c) => (
        <ConflictCard key={c.id} conflict={c} onDismiss={() => dismiss(c.id)} />
      ))}
    </div>
  );
}

function ConflictCard({ conflict, onDismiss }: { conflict: Conflict; onDismiss: () => void }) {
  const windows = navigator.platform.startsWith("Win");
  return (
    <div className="flex gap-2.5 rounded-lg border border-warn/40 bg-warn/10 p-3">
      <TriangleAlert size={16} className="mt-0.5 shrink-0 text-warn" />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="text-sm font-medium">{conflict.name} is running</span>
        <span className="text-xs text-muted">
          It controls {conflict.brands === "every brand" ? "lighting on every brand" : `${conflict.brands} lighting`} too, so colours flicker between it and Lumen, and
          some devices may not show up at all. Quit it from its tray icon
          {windows ? " and turn it off in Task Manager → Startup apps" : " and stop it from starting at login"}. Keep it only if you need its other settings
          (DPI, buttons, firmware).
        </span>
      </div>
      <IconButton label={`Don't warn about ${conflict.name} again`} size="sm" className="shrink-0" onClick={onDismiss}>
        <X size={14} />
      </IconButton>
    </div>
  );
}
