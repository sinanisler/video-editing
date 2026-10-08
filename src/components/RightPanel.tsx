import { useRef, useState } from "react";
import { reader } from "../look";
import { commitProps as commit, core, exec, previewProps as preview, toast, ui } from "../store";
import type { Clip, LogEntry, MediaItem, PropDef, Track, Transition } from "../types";
import { clamp, fpsOf, timecode } from "../util";
import { Tabs } from "./Tabs";

const decimals = (step: number) => (step >= 1 ? 0 : step >= 0.1 ? 1 : 2);

/**
 * A value you can drag or type, with a slider beside it.
 * Drag the number sideways to scrub (Shift for coarse, Alt for fine), click it to type,
 * double-click the label to reset.
 */
function NumberField(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  unit?: string;
  changed?: boolean;
  onLive: (v: number) => void;
  onCommit: (v: number | null) => void;
}) {
  const { label, value, min, max, step, unit, changed, onLive, onCommit } = props;
  const [typing, setTyping] = useState<string | null>(null);
  const latest = useRef(value);
  const digits = decimals(step);
  const snap = (v: number) => clamp(Math.round(v / step) * step, min, max);

  const live = (v: number) => {
    latest.current = snap(v);
    onLive(latest.current);
  };

  const startScrub = (e: React.PointerEvent<HTMLInputElement>) => {
    if (typing !== null || e.button !== 0) return;
    e.preventDefault();
    const x0 = e.clientX;
    const v0 = value;
    // Pixel ranges move one unit per pixel; everything else crosses its range in about 600 px.
    const perPixel = max - min > 1000 ? step : Math.max(step, (max - min) / 600);
    let moved = false;
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - x0;
      if (!moved && Math.abs(dx) < 3) return;
      moved = true;
      live(v0 + dx * perPixel * (ev.shiftKey ? 10 : ev.altKey ? 0.1 : 1));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (moved) onCommit(latest.current);
      else setTyping(value.toFixed(digits));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const finishTyping = (apply: boolean) => {
    const v = Number(typing);
    setTyping(null);
    if (apply && typing?.trim() && !Number.isNaN(v) && snap(v) !== value) onCommit(snap(v));
  };

  return (
    <div className="row num">
      <span className={changed ? "lbl changed" : "lbl"} title="Double-click to reset" onDoubleClick={() => onCommit(null)}>
        {label}
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => live(+e.target.value)}
        onPointerUp={() => onCommit(latest.current)}
        onKeyUp={() => onCommit(latest.current)}
      />
      {typing === null ? (
        <input className="scrub" readOnly value={value.toFixed(digits)} title="Drag to change, click to type" onPointerDown={startScrub} />
      ) : (
        <input
          className="scrub typing"
          autoFocus
          value={typing}
          onFocus={(e) => e.target.select()}
          onChange={(e) => setTyping(e.target.value)}
          onBlur={() => finishTyping(true)}
          onKeyDown={(e) => {
            if (e.key === "Enter") finishTyping(true);
            if (e.key === "Escape") {
              e.stopPropagation();
              finishTyping(false);
            }
          }}
        />
      )}
      <span className="unit">{unit}</span>
    </div>
  );
}

function Row({ label, changed, onReset, children }: { label: string; changed?: boolean; onReset?: () => void; children: React.ReactNode }) {
  return (
    <div className="row">
      <span className={changed ? "lbl changed" : "lbl"} title={onReset && "Double-click to reset"} onDoubleClick={onReset}>
        {label}
      </span>
      {children}
    </div>
  );
}

/** Text that commits on blur or Enter, so typing does not create an undo step per keystroke. */
function TextField({ value, multiline, onCommit }: { value: string; multiline?: boolean; onCommit?: (v: string) => void }) {
  const common = {
    defaultValue: value,
    readOnly: !onCommit,
    onBlur: (e: React.FocusEvent<HTMLInputElement | HTMLTextAreaElement>) => e.target.value !== value && onCommit?.(e.target.value),
  };
  // Re-mount when the stored value changes, so undo and agent edits show up.
  return multiline ? (
    <textarea key={value} rows={2} {...common} />
  ) : (
    <input key={value} type="text" {...common} onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()} />
  );
}

function applies(def: PropDef, c: Clip, track: Track | undefined, m: MediaItem | undefined): boolean {
  switch (def.applies) {
    case "visual":
      return track?.kind !== "audio";
    case "media":
      return !!m && m.kind !== "image";
    case "sound":
      return c.kind === "audio" || m?.kind === "video";
    case "text":
      return c.kind === "graphic" || c.kind === "caption";
  }
}

function TransitionRows({ c, edge, value }: { c: Clip; edge: "in" | "out"; value: Transition | null }) {
  const lib = core.use((s) => s.library)!;
  const set = (transition: string | null, duration?: number) => exec("clip.set_transition", { clip: c.id, edge, transition, duration });
  return (
    <>
      <Row label={edge === "in" ? "At start" : "At end"} changed={!!value} onReset={() => set(null)}>
        <select className="span" value={value?.name ?? "Cut"} onChange={(e) => set(e.target.value === "Cut" ? null : e.target.value, value?.duration)}>
          <option>Cut</option>
          {lib.transitions.map((t) => (
            <option key={t.name}>{t.name}</option>
          ))}
        </select>
      </Row>
      {value && (
        <NumberField
          label="Length"
          unit="fr"
          value={value.duration}
          min={1}
          max={Math.max(1, Math.min(c.duration, 300))}
          step={1}
          onLive={() => {}}
          onCommit={(v) => set(value.name, v ?? undefined)}
        />
      )}
    </>
  );
}

function Inspector() {
  const state = core.use((s) => s.state)!;
  const lib = core.use((s) => s.library)!;
  const selection = ui.use((s) => s.selection);
  const live = ui.use((s) => s.live);
  const p = state.project;
  const chosen = p.sequence.clips.filter((x) => selection.includes(x.id));
  if (chosen.length > 1) {
    return (
      <div className="empty">
        <b>{chosen.length} clips selected</b>
        <p>Drag to move them together, or use the Effects and Transitions tabs to change them all. Select one clip to edit its properties.</p>
      </div>
    );
  }
  const c: Clip | undefined = chosen[0];
  if (!c) return <div className="empty">Select a clip on the timeline to edit its properties.</div>;

  const fps = fpsOf(p);
  const track = p.sequence.tracks.find((t) => t.id === c.track);
  const m = p.media.find((x) => x.id === c.media);
  const r = reader(c, lib, live);
  const defs = lib.properties.filter((d) => applies(d, c, track, m));
  const groups = [...new Set(defs.map((d) => d.group))];
  const isText = c.kind === "graphic" || c.kind === "caption";

  /** Linked zoom changes both axes together, as in Resolve. */
  const withLink = (key: string, value: unknown): Record<string, unknown> =>
    (key === "zoom_x" || key === "zoom_y") && r.b("zoom_link") ? { zoom_x: value, zoom_y: value } : { [key]: value };

  const field = (d: PropDef) => {
    const changed = r.isSet(d.key);
    const reset = () => commit(c.id, withLink(d.key, null));
    switch (d.type) {
      case "number":
        return (
          <NumberField
            key={d.key}
            label={d.label}
            unit={d.unit}
            value={r.n(d.key)}
            min={d.min}
            max={d.max}
            step={d.step}
            changed={changed}
            onLive={(v) => preview(c.id, withLink(d.key, v))}
            onCommit={(v) => commit(c.id, withLink(d.key, v))}
          />
        );
      case "bool":
        return (
          <Row key={d.key} label={d.label} changed={changed} onReset={reset}>
            <input type="checkbox" checked={r.b(d.key)} onChange={(e) => commit(c.id, { [d.key]: e.target.checked })} />
          </Row>
        );
      case "choice":
        return (
          <Row key={d.key} label={d.label} changed={changed} onReset={reset}>
            <select className="span" value={r.s(d.key)} onChange={(e) => commit(c.id, { [d.key]: e.target.value })}>
              {d.options.map((o) => (
                <option key={o}>{o}</option>
              ))}
            </select>
          </Row>
        );
      case "color":
        return (
          <Row key={d.key} label={d.label} changed={changed} onReset={reset}>
            <input
              type="color"
              value={r.s(d.key)}
              onInput={(e) => preview(c.id, { [d.key]: e.currentTarget.value })}
              onChange={(e) => commit(c.id, { [d.key]: e.target.value })}
            />
            <span className="mono dim">{r.s(d.key)}</span>
          </Row>
        );
    }
  };

  return (
    <div key={c.id} className="insp">
      <h4>Clip</h4>
      <Row label="Name">
        <TextField value={c.name} onCommit={(v) => exec("clip.set_property", { clip: c.id, key: "name", value: v })} />
      </Row>
      <Row label="Track">
        <TextField value={`${c.track} · ${track?.name ?? ""}`} />
      </Row>
      <Row label="Start">
        <TextField value={timecode(c.start, fps)} />
      </Row>
      <Row label="Duration">
        <TextField value={timecode(c.duration, fps)} />
      </Row>
      {isText && (
        <Row label="Content">
          <TextField multiline value={c.text ?? ""} onCommit={(v) => exec("clip.set_property", { clip: c.id, key: "text", value: v })} />
        </Row>
      )}

      {groups.map((g) => {
        const inGroup = defs.filter((d) => d.group === g);
        const touched = inGroup.filter((d) => r.isSet(d.key));
        return (
          <section key={g}>
            <h4>
              {g}
              <button
                className="ghost reset"
                disabled={!touched.length}
                title={`Reset ${g.toLowerCase()}`}
                onClick={() => commit(c.id, Object.fromEntries(touched.map((d) => [d.key, null])))}
              >
                ↺
              </button>
            </h4>
            {inGroup.map(field)}
          </section>
        );
      })}

      {track?.kind !== "caption" && (
        <section>
          <h4>Transitions</h4>
          <TransitionRows c={c} edge="in" value={c.transition_in} />
          <TransitionRows c={c} edge="out" value={c.transition_out} />
        </section>
      )}

      {track?.kind !== "audio" && (
        <section>
          <h4>Effects</h4>
          {c.effects.length === 0 && <span className="hint">None. Add from the Effects tab.</span>}
          {c.effects.map((effect) => (
            <div className="fxrow" key={effect}>
              <NumberField
                label={effect}
                value={r.fx(effect) ?? 0}
                min={0}
                max={100}
                step={1}
                changed={r.isSet(`fx.${effect}`)}
                onLive={(v) => preview(c.id, { [`fx.${effect}`]: v })}
                onCommit={(v) => commit(c.id, { [`fx.${effect}`]: v })}
              />
              <button className="ghost" title="Remove effect" onClick={() => exec("clip.remove_effect", { clip: c.id, effect })}>✕</button>
            </div>
          ))}
        </section>
      )}

      <p className="hint note">
        Drag a number sideways to change it (Shift coarse, Alt fine), click it to type, double-click a label to reset.
        {c.last_actor && c.last_actor !== "user" && <span className="agent"> Last changed by {c.last_actor}.</span>}
      </p>
    </div>
  );
}

function Transcript() {
  return (
    <div className="empty">
      <b>No transcript yet</b>
      <p>
        Subtitle generation through OpenRouter arrives with the context milestone. It will fill this panel with
        word-level text, mark silences, and add a captions track.
      </p>
    </div>
  );
}

function LogLine({ e }: { e: LogEntry }) {
  const agent = e.actor !== "user";
  return (
    <div className={`${agent ? "agent" : ""} ${e.ok ? "" : "fail"}`}>
      <span className="who">{agent ? e.actor : "you"}</span> {e.tool}({JSON.stringify(e.args).replace(/"/g, "")})
      <br />
      <span className="res">→ {(typeof e.result === "string" ? e.result : JSON.stringify(e.result).replace(/"/g, "")).slice(0, 400)}</span>
    </div>
  );
}

function Agent() {
  const log = core.use((s) => s.state)!.log;
  const mcp = core.use((s) => s.mcp);
  const connect = `claude mcp add --transport http snn-editor ${mcp?.url ?? ""}`;
  const copy = (text: string) => navigator.clipboard.writeText(text).then(() => toast("Copied"));
  return (
    <>
      <div className="card">
        <div className="line">
          <span className={mcp?.running ? "dot" : "dot off"} />
          <b>MCP server {mcp?.running ? "running" : "not running"}</b>
          <span className="dim">{mcp?.tools} tools</span>
        </div>
        {mcp?.error && <p className="hint err">{mcp.error}. Close whatever uses that port and restart the app.</p>}
        {mcp?.running && (
          <>
            <div className="line">
              <input className="mono" readOnly value={mcp.url} />
              <button onClick={() => copy(mcp.url)}>Copy</button>
            </div>
            <p className="hint">Connect Claude Code with this command, then ask it to edit the open project:</p>
            <div className="line">
              <input className="mono" readOnly value={connect} />
              <button onClick={() => copy(connect)}>Copy</button>
            </div>
            <p className="hint">Only programs on this computer can reach it. Agent edits show up below in purple and can be undone.</p>
          </>
        )}
      </div>
      <h4>Activity</h4>
      <div className="log">
        {log.length === 0 && <p className="hint">No activity yet. Make an edit.</p>}
        {log.map((e) => (
          <LogLine key={e.seq} e={e} />
        ))}
      </div>
    </>
  );
}

export function RightPanel() {
  const tab = ui.use((s) => s.rightTab);
  return (
    <div className="panel" id="right">
      <Tabs
        value={tab}
        onChange={(rightTab) => ui.set({ rightTab })}
        tabs={[
          ["inspector", "Inspector"],
          ["transcript", "Transcript"],
          ["agent", "Agent"],
        ]}
      />
      <div className="pane">
        {tab === "inspector" && <Inspector />}
        {tab === "transcript" && <Transcript />}
        {tab === "agent" && <Agent />}
      </div>
    </div>
  );
}
