import { AnimatePresence, motion } from "motion/react";
import { useEffect } from "react";
import { Rail } from "./components/Rail";
import { TopBar } from "./components/TopBar";
import { CanvasStage } from "./features/canvas/CanvasStage";
import { DevicesPanel } from "./features/devices/DevicesPanel";
import { LayersPanel } from "./features/effects/LayersPanel";
import { ProfilesPanel } from "./features/profiles/ProfilesPanel";
import { SettingsPanel } from "./features/settings/SettingsPanel";
import { useStore } from "./store";

const panels = {
  layers: LayersPanel,
  devices: DevicesPanel,
  profiles: ProfilesPanel,
  settings: SettingsPanel,
} as const;

export default function App() {
  const init = useStore((s) => s.init);
  const tab = useStore((s) => s.tab);
  useEffect(() => {
    init();
  }, [init]);

  const Panel = panels[tab];

  return (
    <div className="grid h-full grid-rows-[52px_1fr] bg-bg">
      <TopBar />
      <div className="grid min-h-0 grid-cols-[60px_1fr_380px]">
        <Rail />
        <CanvasStage />
        <aside className="min-h-0 overflow-y-auto border-l border-border bg-surface">
          <AnimatePresence mode="wait" initial={false}>
            <motion.div key={tab} initial={{ opacity: 0, x: 10 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: 10 }} transition={{ duration: 0.14 }}>
              <Panel />
            </motion.div>
          </AnimatePresence>
        </aside>
      </div>
    </div>
  );
}
