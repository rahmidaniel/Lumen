import { Bookmark, Cpu, Layers, Settings } from "lucide-react";
import { motion } from "motion/react";
import { useStore, type Tab } from "../store";
import { cx } from "./ui";

const items: { tab: Tab; label: string; icon: typeof Layers }[] = [
  { tab: "layers", label: "Effects", icon: Layers },
  { tab: "devices", label: "Devices", icon: Cpu },
  { tab: "profiles", label: "Profiles", icon: Bookmark },
  { tab: "settings", label: "Settings", icon: Settings },
];

export function Rail() {
  const tab = useStore((s) => s.tab);
  const setTab = useStore((s) => s.setTab);
  return (
    <nav className="flex flex-col items-center gap-1 py-3 border-r border-border bg-surface">
      {items.map(({ tab: t, label, icon: Icon }) => {
        const active = t === tab;
        return (
          <button
            key={t}
            onClick={() => setTab(t)}
            title={label}
            aria-label={label}
            className={cx(
              "relative flex h-11 w-11 items-center justify-center rounded-lg transition-colors",
              active ? "text-text" : "text-faint hover:text-muted hover:bg-surface-2",
            )}
          >
            {active && <motion.span layoutId="rail-active" className="absolute inset-0 rounded-lg bg-accent-soft" transition={{ type: "spring", stiffness: 500, damping: 40 }} />}
            <Icon size={18} className="relative" />
          </button>
        );
      })}
    </nav>
  );
}
