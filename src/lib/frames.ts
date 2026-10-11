// Preview frames arrive ~30×/s. They bypass React state: the canvas reads the
// latest one inside requestAnimationFrame and nothing else re-renders.

import type { FramePayload } from "./types";

type Listener = (frame: FramePayload) => void;

let latest: FramePayload | null = null;
const listeners = new Set<Listener>();

export const frames = {
  get latest(): FramePayload | null {
    return latest;
  },
  push(frame: FramePayload) {
    latest = frame;
    for (const l of listeners) l(frame);
  },
  subscribe(listener: Listener): () => void {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
};
