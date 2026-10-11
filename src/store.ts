import { create } from "zustand";
import { frames } from "./lib/frames";
import { getBackend, type Backend } from "./lib/ipc";
import type { Effect, Layer, OnboardEffect, OpenRgbStatus, Placement, Scene, Snapshot } from "./lib/types";

export type Tab = "devices" | "layers" | "profiles" | "settings";

interface State {
  ready: boolean;
  snapshot: Snapshot | null;
  openrgb: OpenRgbStatus | null;
  profiles: string[];
  tab: Tab;
  selectedLayer: string | null;
  selectedDevice: number | null;
  error: string | null;

  init(): Promise<void>;
  setTab(tab: Tab): void;
  selectLayer(id: string | null): void;
  selectDevice(id: number | null): void;

  setLayers(layers: Layer[]): void;
  patchLayer(id: string, patch: Partial<Layer>): void;
  addLayer(effect: Effect, name: string): void;
  removeLayer(id: string): void;
  moveLayer(id: string, delta: -1 | 1): void;

  /** Update a device's placement; `commit` false while dragging (throttled), true on release. */
  setPlacement(deviceId: number, placement: Placement, commit: boolean): void;
  setBrightness(brightness: number): void;
  setPaused(paused: boolean): void;
  setFps(fps: number): void;
  refreshDevices(): Promise<void>;
  saveToDevice(deviceId: number, effect: OnboardEffect): Promise<void>;
  saveProfile(name: string): Promise<void>;
  loadProfile(name: string): Promise<void>;
  deleteProfile(name: string): Promise<void>;
  startOpenrgb(): Promise<void>;
}

let backend: Backend | null = null;
const api = () => {
  if (!backend) throw new Error("backend not ready");
  return backend;
};

const fail = (set: (p: Partial<State>) => void) => (e: unknown) => set({ error: e instanceof Error ? e.message : String(e) });

let layoutTimer: number | null = null;
let pendingLayout: Scene["layout"] | null = null;
function pushLayout(layout: Scene["layout"], commit: boolean, onError: (e: unknown) => void) {
  pendingLayout = layout;
  if (commit) {
    if (layoutTimer !== null) window.clearTimeout(layoutTimer);
    layoutTimer = null;
    api().setLayout(layout).catch(onError);
    pendingLayout = null;
  } else if (layoutTimer === null) {
    layoutTimer = window.setTimeout(() => {
      layoutTimer = null;
      if (pendingLayout) api().setLayout(pendingLayout).catch(onError);
      pendingLayout = null;
    }, 40);
  }
}

function withScene(set: (fn: (s: State) => Partial<State>) => void, update: (scene: Scene) => Scene) {
  set((s) => (s.snapshot ? { snapshot: { ...s.snapshot, scene: update(s.snapshot.scene) } } : {}));
}

export const useStore = create<State>()((set, get) => ({
  ready: false,
  snapshot: null,
  openrgb: null,
  profiles: [],
  tab: "devices",
  selectedLayer: null,
  selectedDevice: null,
  error: null,

  async init() {
    if (get().ready) return;
    backend = await getBackend();
    backend.onSnapshot((snapshot) => {
      set((s) => ({
        snapshot,
        selectedLayer:
          s.selectedLayer && snapshot.scene.layers.some((l) => l.id === s.selectedLayer)
            ? s.selectedLayer
            : (snapshot.scene.layers.at(-1)?.id ?? null),
      }));
    });
    backend.onFrame((f) => frames.push(f));
    backend.onOpenrgbStatus((openrgb) => set({ openrgb }));
    const [snapshot, openrgb, profiles] = await Promise.all([
      backend.getSnapshot(),
      backend.openrgbStatus().catch(() => null),
      backend.listProfiles().catch(() => [] as string[]),
    ]);
    set({ ready: true, snapshot, openrgb, profiles, selectedLayer: snapshot.scene.layers.at(-1)?.id ?? null });
  },

  setTab: (tab) => set({ tab }),
  selectLayer: (id) => set({ selectedLayer: id, tab: "layers" }),
  selectDevice: (id) => set({ selectedDevice: id }),

  setLayers(layers) {
    withScene(set, (scene) => ({ ...scene, layers }));
    api().setLayers(layers).catch(fail(set));
  },
  patchLayer(id, patch) {
    const layers = get().snapshot?.scene.layers.map((l) => (l.id === id ? { ...l, ...patch } : l));
    if (layers) get().setLayers(layers);
  },
  addLayer(effect, name) {
    const layer: Layer = {
      id: crypto.randomUUID(),
      name,
      enabled: true,
      effect,
      opacity: 1,
      blend: "normal",
      mask: { type: "all" },
    };
    const layers = [...(get().snapshot?.scene.layers ?? []), layer];
    get().setLayers(layers);
    set({ selectedLayer: layer.id });
  },
  removeLayer(id) {
    const layers = get().snapshot?.scene.layers.filter((l) => l.id !== id);
    if (!layers) return;
    get().setLayers(layers);
    if (get().selectedLayer === id) set({ selectedLayer: layers.at(-1)?.id ?? null });
  },
  moveLayer(id, delta) {
    const layers = [...(get().snapshot?.scene.layers ?? [])];
    const i = layers.findIndex((l) => l.id === id);
    const j = i + delta;
    if (i < 0 || j < 0 || j >= layers.length) return;
    [layers[i], layers[j]] = [layers[j], layers[i]];
    get().setLayers(layers);
  },

  setPlacement(deviceId, placement, commit) {
    let layout: Scene["layout"] | null = null;
    withScene(set, (scene) => {
      layout = { placements: { ...scene.layout.placements, [String(deviceId)]: placement } };
      return { ...scene, layout };
    });
    if (layout) pushLayout(layout, commit, fail(set));
  },
  setBrightness(brightness) {
    withScene(set, (scene) => ({ ...scene, brightness }));
    api().setBrightness(brightness).catch(fail(set));
  },
  setPaused(paused) {
    set((s) => (s.snapshot ? { snapshot: { ...s.snapshot, paused } } : {}));
    api().setPaused(paused).catch(fail(set));
  },
  setFps(fps) {
    set((s) => (s.snapshot ? { snapshot: { ...s.snapshot, fps } } : {}));
    api().setFps(fps).catch(fail(set));
  },
  async refreshDevices() {
    await api().refreshDevices().catch(fail(set));
  },
  async saveToDevice(deviceId, effect) {
    await api().saveToDevice(deviceId, effect);
  },
  async saveProfile(name) {
    const profiles = await api().saveProfile(name);
    set({ profiles });
  },
  async loadProfile(name) {
    await api().loadProfile(name);
  },
  async deleteProfile(name) {
    const profiles = await api().deleteProfile(name);
    set({ profiles });
  },
  async startOpenrgb() {
    set({ error: null });
    try {
      const openrgb = await api().startOpenrgb();
      set({ openrgb });
    } catch (e) {
      fail(set)(e);
      set({ openrgb: await api().openrgbStatus().catch(() => null) });
    }
  },
}));

export const useSnapshot = () => useStore((s) => s.snapshot);
export const useScene = () => useStore((s) => s.snapshot?.scene ?? null);
