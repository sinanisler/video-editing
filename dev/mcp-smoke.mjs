// End-to-end check of the MCP server, shaped like a real edit: cut dead air out of a long
// recording, build a cold open in front of it, add cards and captions, then undo everything.
//
//   node dev/mcp-smoke.mjs            needs the app running with an empty, unsaved project
//   node dev/mcp-smoke.mjs --keep     leave the result on the timeline to look at
//
// It refuses to run on a project that has anything in it, because it edits the open project.

const URL = process.env.SNN_MCP ?? "http://127.0.0.1:7420/mcp";
const KEEP = process.argv.includes("--keep");

let session;
let nextId = 1;
let steps = 0; // undo steps this script has added
const results = { pass: 0, fail: 0, warn: 0 };

function report(kind, label, detail) {
  results[kind]++;
  console.log(`${{ pass: "ok  ", fail: "FAIL", warn: "warn" }[kind]}  ${label}${detail ? `: ${detail}` : ""}`);
}
const check = (label, ok, detail) => report(ok ? "pass" : "fail", label, ok ? "" : detail);

async function post(body, headers = {}, path = "") {
  const h = { "Content-Type": "application/json", Accept: "application/json, text/event-stream", ...headers };
  if (session) h["Mcp-Session-Id"] = session;
  const r = await fetch(URL + path, { method: "POST", headers: h, body: typeof body === "string" ? body : JSON.stringify(body) });
  session = r.headers.get("mcp-session-id") ?? session;
  const text = await r.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {}
  return { status: r.status, json };
}

const rpc = (method, params = {}) => post({ jsonrpc: "2.0", id: nextId++, method, params });

/** Runs a tool. Returns `{ ok, data }`, where data is the parsed result or the error text. */
async function tool(name, args = {}) {
  const r = await rpc("tools/call", { name, arguments: args });
  const result = r.json?.result;
  if (!result) return { ok: false, data: JSON.stringify(r.json?.error ?? r.status) };
  const text = result.content?.[0]?.text ?? "";
  if (result.isError) return { ok: false, data: text };
  try {
    return { ok: true, data: JSON.parse(text) };
  } catch {
    return { ok: true, data: text }; // guides come back as Markdown
  }
}

/** An edit that is expected to work. Counts the undo step it adds. */
async function edit(name, args) {
  const r = await tool(name, args);
  if (r.ok) steps++;
  else report("fail", name, r.data);
  return r.data;
}

async function rejected(label, name, args) {
  const before = JSON.stringify((await tool("timeline_get_state")).data);
  const r = await tool(name, args);
  const after = JSON.stringify((await tool("timeline_get_state")).data);
  check(label, !r.ok && before === after, r.ok ? "the command was accepted" : "the project changed");
}

const clipsOn = async (track) => (await tool("timeline_get_state", { track })).data.clips;

async function main() {
  // --- Protocol ---------------------------------------------------------------------------
  const init = await rpc("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "mcp-smoke", version: "1" } });
  check("initialize", init.status === 200 && init.json?.result?.serverInfo?.name === "snn-editor", JSON.stringify(init.json));
  check("session id issued", !!session);
  check("instructions sent", (init.json?.result?.instructions ?? "").length > 50);
  check("notification gets 202", (await post({ jsonrpc: "2.0", method: "notifications/initialized" })).status === 202);
  check("ping", !!(await rpc("ping")).json?.result);

  const listed = (await rpc("tools/list")).json?.result?.tools ?? [];
  check("tools/list", listed.length > 20, `${listed.length} tools`);
  const badNames = listed.filter((t) => !/^[a-zA-Z0-9_-]{1,64}$/.test(t.name)).map((t) => t.name);
  check("tool names valid for MCP clients", badNames.length === 0, badNames.join(", "));
  const noSchema = listed.filter((t) => t.inputSchema?.type !== "object").map((t) => t.name);
  check("every tool has an object schema", noSchema.length === 0, noSchema.join(", "));

  const prompts = (await rpc("prompts/list")).json?.result?.prompts ?? [];
  check("guides are offered as prompts", prompts.some((p) => p.name === "retention-edit"), `${prompts.length} prompts`);
  const prompt = (await rpc("prompts/get", { name: "cold-open" })).json?.result?.messages?.[0]?.content?.text ?? "";
  check("a prompt carries the guide text", prompt.includes("# Cold open"));
  const guides = (await tool("guide_list")).data.guides ?? [];
  check("guide_list", guides.length === prompts.length && guides.every((g) => g.description));
  const guide = (await tool("guide_get", { name: "retention-edit" })).data;
  check("guide_get returns readable Markdown", typeof guide === "string" && guide.startsWith("# ") && guide.includes("\n"));

  check("unknown method is an RPC error", (await rpc("nope/nope")).json?.error?.code === -32601);
  check("malformed JSON gets 400", (await post("{not json")).status === 400);
  check("wrong path gets 404", (await post({}, {}, "/x")).status === 404);
  check("foreign browser origin gets 403", (await post({ jsonrpc: "2.0", id: nextId++, method: "ping" }, { Origin: "https://example.com" })).status === 403);
  check("GET gets 405", (await fetch(URL)).status === 405);
  check("unknown tool is a tool error", !(await tool("timeline_explode")).ok);

  // --- Guard ------------------------------------------------------------------------------
  const info = (await tool("project_get_info")).data;
  if (info.clips || info.media || info.markers) {
    console.log(`\nThe open project "${info.name}" is not empty (${info.clips} clips, ${info.media} media, ${info.markers} markers).`);
    console.log("Stopping before any edit. Run this against a new, empty project.");
    return summary();
  }
  const fps = info.fps;
  const sec = (s) => Math.round(s * fps);

  // --- Media and a long recording ----------------------------------------------------------
  // Turkish characters in the path and a file that does not exist: the pool only records it.
  const path = "C:/Kayıtlar/Zindan Bölüm 1 çğışöü.mp4";
  const RAW = sec(3600);
  const media = (await edit("media_import", { path, kind: "video", duration: RAW, width: 1920, height: 1080 })).media;
  const pool = (await tool("media_list")).data.media;
  check("Turkish path survives the round trip", pool[0]?.path === path, pool[0]?.path);
  const again = await tool("media_import", { path, kind: "video", duration: RAW });
  if (again.ok) steps++;
  check("re-import returns the same item", again.data?.already_imported === true && again.data.media.id === media.id);

  await edit("timeline_rename_track", { track: "V1", name: "Gameplay" });
  const raw = (await edit("timeline_add_clip", { track: "V1", start: 0, duration: RAW, name: "Raw", kind: "video", media: media.id })).clip;
  await rejected("clip past the end of its source is rejected", "timeline_add_clip", { track: "V2", start: 0, duration: RAW + 1, name: "x", kind: "video", media: media.id });
  await rejected("overlap is rejected", "timeline_add_clip", { track: "V1", start: 10, duration: 10, name: "x", kind: "video", media: media.id });
  await rejected("wrong argument type is rejected", "timeline_split_clip", { at: "soon" });
  await rejected("video clip on an audio track is rejected", "timeline_add_clip", { track: "A1", start: 0, duration: 10, name: "x", kind: "video", media: media.id });

  // --- Dead-air pass: 60 cuts in one undo step -----------------------------------------------
  // Each cut is [from, to) in the timeline as it is now. They are sent last to first, so removing
  // one does not move the ones still to come.
  const cuts = [];
  for (let i = 0; i < 60; i++) {
    const from = sec(20 + i * 58);
    cuts.push([from, from + sec(4 + (i % 7) * 3)]);
  }
  const removed = cuts.reduce((n, [a, b]) => n + (b - a), 0);
  const t0 = performance.now();
  await edit("batch_apply", { commands: [...cuts].reverse().map(([from, to]) => ({ tool: "timeline_ripple_delete_range", args: { from, to } })) });
  const batchMs = performance.now() - t0;

  let v1 = await clipsOn("V1");
  check("60 cuts leave 61 pieces", v1.length === 61, `${v1.length} pieces`);
  check("length shrinks by exactly what was cut", v1.at(-1).start + v1.at(-1).duration === RAW - removed);
  let gapless = true;
  let sourceRight = true;
  let at = 0;
  let src = 0;
  v1.forEach((c, i) => {
    if (c.start !== at) gapless = false;
    if (c.source_in !== src) sourceRight = false;
    at += c.duration;
    src += c.duration + (i < cuts.length ? cuts[i][1] - cuts[i][0] : 0);
  });
  check("no gaps after the cuts", gapless);
  check("every piece still shows the right part of the recording", sourceRight);
  report(batchMs < 3000 ? "pass" : "warn", "batch of 60 cuts", `${Math.round(batchMs)} ms`);

  // --- Cold open in front of the video -------------------------------------------------------
  const marker = (await edit("timeline_add_marker", { at: sec(60), label: "check this" })).marker;
  const OPEN = sec(12);
  await edit("timeline_move_clips", { clips: v1.map((c) => c.id), delta: OPEN });
  const moved = (await tool("timeline_get_state", { from: 0, to: 1 })).data.markers.find((m) => m.id === marker);
  if (moved?.at === sec(60)) report("warn", "markers stay behind when clips are shifted", "re-place markers after making room for an intro");

  // Three teaser shots copied from later in the recording, 1.5 s each, after a 4 s promise shot.
  const picks = [sec(3000), sec(1200), sec(2400), sec(3500)];
  const lens = [sec(4), sec(1.5), sec(1.5), sec(1.5)];
  const teaser = [];
  let head = 0;
  for (let i = 0; i < picks.length; i++) {
    teaser.push({ tool: "timeline_add_clip", args: { track: "V1", start: head, duration: lens[i], name: `Teaser ${i + 1}`, kind: "video", media: media.id, source_in: picks[i] } });
    head += lens[i];
  }
  const made = (await edit("batch_apply", { commands: teaser })).results.map((r) => r.clip);
  v1 = await clipsOn("V1");
  check("teaser clips keep their source positions", made.every((id, i) => v1.find((c) => c.id === id)?.source_in === picks[i]));
  check("the body follows the cold open", v1.find((c) => c.id === raw)?.start === OPEN);

  const title = (await edit("timeline_add_clip", { track: "V3", start: 0, duration: sec(4), name: "Title", kind: "graphic", template: "title", text: "MINECRAFT'I RPG YAPTIM" })).clip;
  const caption = (await edit("timeline_add_clip", { track: "T1", start: 0, duration: sec(1), name: "cap", kind: "caption", text: "ÖLÜRSEM HER ŞEYİMİ" })).clip;
  await edit("batch_apply", {
    commands: [
      { tool: "timeline_add_clip", args: { track: "V3", start: sec(14), duration: sec(3), name: "Subscribe", kind: "graphic", template: "subscribe", text: "ABONE OL" } },
      { tool: "timeline_add_clip", args: { track: "V2", start: sec(20), duration: sec(30), name: "Counter", kind: "graphic", template: "counter", text: "TIER 1" } },
      { tool: "clip_add_effect", args: { clip: made[0], effect: "Zoom punch" } },
      { tool: "clip_set_property", args: { clip: made[0], key: "fx.Zoom punch", value: 60 } },
      { tool: "clip_set_transition", args: { clip: made[1], transition: "Whoosh zoom", duration: 6 } },
      { tool: "clip_set_property", args: { clip: caption, key: "stroke_size", value: 12 } },
      { tool: "clip_set_property", args: { clip: title, key: "font", value: "Impact" } },
    ],
  });
  const shot = (await tool("timeline_get_clip", { clip: made[0] })).data;
  check("effect and its strength are stored", shot.effects.includes("Zoom punch") && shot.props["fx.Zoom punch"] === 60);
  check("Turkish text survives the round trip", (await tool("timeline_get_clip", { clip: caption })).data.text === "ÖLÜRSEM HER ŞEYİMİ");
  await rejected("unknown effect is rejected", "clip_add_effect", { clip: made[0], effect: "Sparkle" });
  await rejected("out-of-range property is rejected", "clip_set_property", { clip: made[0], key: "opacity", value: 400 });
  await rejected("failed batch changes nothing", "batch_apply", { commands: [{ tool: "timeline_add_marker", args: { at: 5, label: "a" } }, { tool: "timeline_delete_clip", args: { clip: "c999999" } }] });

  const pace = (await tool("timeline_get_pace")).data;
  check("pace sees the cold open", pace.first_30_seconds >= 5 && pace.per_minute.length === Math.ceil(pace.duration_frames / sec(60)), JSON.stringify(pace.per_minute?.slice(0, 3)));
  console.log(`      pace: ${pace.first_30_seconds} changes in the first 30 s, per minute ${pace.per_minute.slice(0, 6).join(" ")} …`);

  // --- Reads the agent leans on ---------------------------------------------------------------
  const t1 = performance.now();
  const whole = await rpc("tools/call", { name: "timeline_get_state", arguments: {} });
  const wholeKb = Math.round(JSON.stringify(whole.json).length / 1024);
  const windowed = await rpc("tools/call", { name: "timeline_get_state", arguments: { from: 0, to: sec(90) } });
  const windowKb = Math.round(JSON.stringify(windowed.json).length / 1024);
  console.log(`      timeline_get_state: whole project ${wholeKb} KB in ${Math.round(performance.now() - t1)} ms, first 90 s ${windowKb} KB`);
  const log = (await tool("history_list", { limit: 5 })).data.entries;
  check("edits are logged under the client's name", log.some((e) => e.actor === "mcp-smoke"), log[0]?.actor);

  // --- Undo everything -------------------------------------------------------------------------
  if (KEEP) {
    console.log(`\nLeft on the timeline (--keep). ${steps} undo steps remove it.`);
    return summary();
  }
  for (let i = 0; i < steps; i++) await tool("history_undo");
  const end = (await tool("project_get_info")).data;
  check(`undoing ${steps} steps restores the empty project`, end.clips === 0 && end.media === 0 && end.markers === 0, JSON.stringify(end));
  summary();
}

function summary() {
  console.log(`\n${results.pass} passed, ${results.fail} failed, ${results.warn} warnings`);
  process.exitCode = results.fail ? 1 : 0;
}

main().catch((e) => {
  const refused = e.cause?.code === "ECONNREFUSED";
  console.log(refused ? `Nothing is listening on ${URL}. Start the app with: npm run tauri dev` : `Stopped: ${e.stack ?? e}`);
  process.exitCode = 1;
});
