import { FolderOpen, Save, Trash2 } from "lucide-react";
import { useState } from "react";
import { Button, IconButton, Panel, TextInput } from "../../components/ui";
import { useStore } from "../../store";

export function ProfilesPanel() {
  const profiles = useStore((s) => s.profiles);
  const save = useStore((s) => s.saveProfile);
  const load = useStore((s) => s.loadProfile);
  const remove = useStore((s) => s.deleteProfile);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState<string | null>(null);

  const run = async (key: string, fn: () => Promise<void>) => {
    setBusy(key);
    try {
      await fn();
    } finally {
      setBusy(null);
    }
  };

  return (
    <Panel title="Profiles">
      <p className="text-xs text-muted">A profile is a snapshot of the whole scene: layout, layers and brightness. The current scene is always restored on launch.</p>
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          const n = name.trim();
          if (!n) return;
          run("save", () => save(n)).then(() => setName(""));
        }}
      >
        <TextInput placeholder="Profile name" value={name} onChange={(e) => setName(e.target.value)} />
        <Button type="submit" variant="primary" disabled={!name.trim() || busy === "save"}>
          <Save size={14} /> Save
        </Button>
      </form>

      {profiles.length === 0 && <p className="text-sm text-muted">No saved profiles yet.</p>}
      <ul className="flex flex-col gap-1">
        {profiles.map((p) => (
          <li key={p} className="flex items-center gap-2 rounded-lg border border-border bg-surface-2 px-3 py-2">
            <span className="min-w-0 flex-1 truncate text-sm">{p}</span>
            <Button size="sm" disabled={busy !== null} onClick={() => run(`load:${p}`, () => load(p))}>
              <FolderOpen size={14} /> Load
            </Button>
            <IconButton label={`Delete ${p}`} size="sm" className="hover:text-danger" disabled={busy !== null} onClick={() => run(`del:${p}`, () => remove(p))}>
              <Trash2 size={14} />
            </IconButton>
          </li>
        ))}
      </ul>
    </Panel>
  );
}
