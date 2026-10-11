import { ChevronDown, ChevronUp, Eye, EyeOff, Plus, Trash2 } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { Button, IconButton, Panel, cx } from "../../components/ui";
import type { EffectType, Layer } from "../../lib/types";
import { useStore } from "../../store";
import { EffectEditor, defaultEffect, effectLabels } from "./EffectEditor";

// Stable fallback: a selector must not return a new array on every call.
const NO_LAYERS: Layer[] = [];

export function LayersPanel() {
  const layers = useStore((s) => s.snapshot?.scene.layers) ?? NO_LAYERS;
  const selected = useStore((s) => s.selectedLayer);
  const select = useStore((s) => s.selectLayer);
  const patch = useStore((s) => s.patchLayer);
  const add = useStore((s) => s.addLayer);
  const remove = useStore((s) => s.removeLayer);
  const move = useStore((s) => s.moveLayer);
  const [adding, setAdding] = useState(false);

  // Topmost layer first, like a design tool.
  const ordered = [...layers].reverse();
  const current = layers.find((l) => l.id === selected) ?? null;

  return (
    <Panel
      title="Effects"
      actions={
        <Button size="sm" variant="primary" onClick={() => setAdding((v) => !v)}>
          <Plus size={14} /> Add layer
        </Button>
      }
    >
      <AnimatePresence initial={false}>
        {adding && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.18 }}
            className="overflow-hidden"
          >
            <div className="grid grid-cols-2 gap-1.5 rounded-lg border border-border bg-surface-2 p-2">
              {(Object.keys(effectLabels) as EffectType[]).map((t) => (
                <button
                  key={t}
                  onClick={() => {
                    add(defaultEffect(t), effectLabels[t]);
                    setAdding(false);
                  }}
                  className="rounded-md px-2.5 py-2 text-left text-sm hover:bg-surface-3"
                >
                  {effectLabels[t]}
                </button>
              ))}
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {layers.length === 0 && <p className="text-sm text-muted">No layers. Add one to start painting your devices.</p>}

      <ul className="flex flex-col gap-1">
        <AnimatePresence initial={false}>
          {ordered.map((layer, i) => {
            const active = layer.id === selected;
            return (
              <motion.li
                key={layer.id}
                layout
                initial={{ opacity: 0, y: -4 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, scale: 0.98 }}
                transition={{ duration: 0.15 }}
                onClick={() => select(layer.id)}
                className={cx(
                  "group flex cursor-pointer items-center gap-2 rounded-lg border px-2 py-1.5 transition-colors",
                  active ? "border-accent/60 bg-accent-soft/40" : "border-border bg-surface-2 hover:bg-surface-3",
                  !layer.enabled && "opacity-60",
                )}
              >
                <IconButton
                  label={layer.enabled ? "Hide layer" : "Show layer"}
                  size="sm"
                  onClick={(e) => {
                    e.stopPropagation();
                    patch(layer.id, { enabled: !layer.enabled });
                  }}
                >
                  {layer.enabled ? <Eye size={14} /> : <EyeOff size={14} />}
                </IconButton>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{layer.name}</div>
                  <div className="truncate text-[11px] text-muted">
                    {effectLabels[layer.effect.type]} · {Math.round(layer.opacity * 100)}%{layer.blend !== "normal" ? ` · ${layer.blend}` : ""}
                    {layer.mask.type !== "all" ? " · partial" : ""}
                  </div>
                </div>
                <div className="flex items-center opacity-0 transition-opacity group-hover:opacity-100">
                  <IconButton label="Move up" size="sm" disabled={i === 0} onClick={(e) => (e.stopPropagation(), move(layer.id, 1))}>
                    <ChevronUp size={14} />
                  </IconButton>
                  <IconButton label="Move down" size="sm" disabled={i === ordered.length - 1} onClick={(e) => (e.stopPropagation(), move(layer.id, -1))}>
                    <ChevronDown size={14} />
                  </IconButton>
                  <IconButton label="Delete layer" size="sm" className="hover:text-danger" onClick={(e) => (e.stopPropagation(), remove(layer.id))}>
                    <Trash2 size={14} />
                  </IconButton>
                </div>
              </motion.li>
            );
          })}
        </AnimatePresence>
      </ul>

      {current && <EffectEditor key={current.id} layer={current} />}
    </Panel>
  );
}
