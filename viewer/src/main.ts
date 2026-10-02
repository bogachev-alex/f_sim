import "./style.css";
import { Camera } from "./camera";
import { emptySnapshot, FrameStore, type Snapshot } from "./frames";
import type { MatchInfo } from "./generated/MatchInfo";
import { Playback } from "./playback";
import type { FromWorker, ToWorker } from "./protocol";
import { Renderer } from "./render";
import { teamShape } from "./shape";
import { Ui } from "./ui";
import Worker_ from "./worker?worker";

/** Сколько секунд матча хранить позади для следов и перемотки на шаг. */
const HISTORY_SECONDS = 8;
/** Ограничение шага по времени кадра, чтобы после простоя вкладки игра не прыгала. */
const MAX_FRAME_DT_S = 0.1;

const canvas = document.getElementById("pitch") as HTMLCanvasElement;
const stage = document.getElementById("stage") as HTMLElement;
const hud = document.getElementById("hud") as HTMLElement;
const caption = document.getElementById("caption") as HTMLElement;
let descriptions = new Map<string, string>();
const scenarioDescription = (id: string) => descriptions.get(id) ?? "";
const worker = new Worker_();
const post = (m: ToWorker) => worker.postMessage(m);

interface Session {
  info: MatchInfo;
  store: FrameStore;
  playback: Playback;
  camera: Camera;
  renderer: Renderer;
  snap: Snapshot;
  requested: number;
}

let session: Session | null = null;
let gen = 0;
let lastError = "";
let fps = 60;
let pendingStart = false;
let snapped = false;
interface MatchStats {
  passes: [number, number];
  passes_ok: [number, number];
  shots: [number, number];
  shots_on_target: [number, number];
  xg: [number, number];
  possessions: [number, number];
  tackles_won: [number, number];
  interceptions: [number, number];
  score: [number, number];
}
let stats: MatchStats | null = null;

const ui = new Ui(document.getElementById("toolbar") as HTMLElement, {
  onPlayPause() {
    ui.state.paused = !ui.state.paused;
    if (session) session.playback.paused = ui.state.paused;
    ui.refresh();
  },
  onStep() {
    ui.state.paused = true;
    if (session) {
      session.playback.stepOnce(session.store.latestTick());
      requestFrames(session);
    }
    ui.refresh();
  },
  onSpeed(speed) {
    if (session) session.playback.speed = speed;
  },
  onCamera(mode) {
    if (session) session.camera.mode = mode;
  },
  onNewMatch: () => startMatch(),
  onRestart: () => startMatch(),
});

function startMatch() {
  ui.readMatchParams();
  gen += 1;
  lastError = "";
  session = null;
  const { mode, scenario, seed, quality, styleA, styleB, formationA, formationB } = ui.state;
  post({ type: "new", gen, mode, scenario, seed, quality, styleA, styleB, formationA, formationB });
}

worker.onmessage = (e: MessageEvent<FromWorker>) => {
  const m = e.data;
  if (m.type === "init") {
    ui.setCatalog(m.catalog);
    descriptions = new Map(m.catalog.scenarios.map((x) => [x.id, x.description]));
    pendingStart = true;
    startMatch();
    return;
  }
  if (m.gen !== gen) return; // кадры от прошлого матча
  if (m.type === "error") {
    lastError = m.message;
  } else if (m.type === "ready") {
    snapped = false;
    const playback = new Playback(m.info.tick_hz);
    playback.speed = ui.state.speed;
    playback.paused = ui.state.paused;
    const camera = new Camera(m.info.pitch.length_m, m.info.pitch.width_m);
    camera.mode = ui.state.camera;
    session = {
      info: m.info,
      store: new FrameStore(m.info.layout, m.info.tick_hz),
      playback,
      camera,
      renderer: new Renderer(canvas, m.info),
      snap: emptySnapshot(m.info.layout.players),
      requested: 0,
    };
  } else if (m.type === "frames" && session) {
    session.store.push(new Float32Array(m.buffer));
    stats = m.stats && m.stats !== "null" ? (JSON.parse(m.stats) as MatchStats) : null;
  }
};

/** Просит у воркера кадры с запасом вперёд. */
function requestFrames(s: Session) {
  const need = Math.ceil(s.playback.tick) + s.playback.leadTicks();
  if (s.requested < need) {
    s.requested = need + s.playback.leadTicks();
    post({ type: "run_to", gen, tick: s.requested });
  }
}

function resize() {
  const dpr = window.devicePixelRatio || 1;
  const r = stage.getBoundingClientRect();
  const w = Math.max(1, Math.round(r.width * dpr));
  const h = Math.max(1, Math.round(r.height * dpr));
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
}
new ResizeObserver(resize).observe(stage);
resize();

let last = performance.now();
function frame(now: number) {
  const dt = Math.min((now - last) / 1000, MAX_FRAME_DT_S);
  last = now;
  fps += (1 / Math.max(dt, 1e-3) - fps) * 0.05;
  const s = session;
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.width / dpr;
  const h = canvas.height / dpr;
  if (s && s.store.length > 0) {
    s.playback.advance(dt, s.store.latestTick());
    requestFrames(s);
    s.store.sample(s.playback.tick, s.snap);
    if (!snapped || s.playback.tick === 0) {
      s.camera.snap(w, h, s.snap.ball.x, s.snap.ball.y);
      snapped = true;
    } else {
      s.camera.update(dt, w, h, s.snap.ball.x, s.snap.ball.y);
    }
    s.renderer.draw(s.snap, s.camera, s.store, s.playback.tick, { ...ui.state.layers });
    s.store.prune(s.playback.tick, HISTORY_SECONDS * s.info.tick_hz);
    const ended = s.info.duration_ticks > 0 && s.playback.tick >= s.info.duration_ticks - 0.5;
    const stalled = s.playback.tick >= s.store.latestTick() && !s.playback.paused && !ended;
    const lines = [
      `${fmtTime(s.playback.seconds())}  тик ${Math.floor(s.playback.tick)}  ${s.playback.speed}×` +
        (ended ? "  сценарий завершён: «Перезапуск»" : ""),
      `${fps.toFixed(0)} FPS  кадров ${s.store.length}${stalled ? "  ждём движок" : ""}`,
    ];
    if (s.info.mode === "match" && stats) {
      const acc = (k: number) => (stats!.passes[k]! > 0 ? Math.round((100 * stats!.passes_ok[k]!) / stats!.passes[k]!) : 0);
      lines.push(`Счёт ${stats.score[0]}:${stats.score[1]}  A ${s.info.styles[0]} · B ${s.info.styles[1]}`);
      for (let k = 0; k < 2; k++) {
        lines.push(
          `${"AB"[k]}: передач ${stats.passes[k]} (${acc(k)}%), перехватов ${stats.interceptions[k]}, отборов ${stats.tackles_won[k]}, ` +
            `ударов ${stats.shots[k]} (в створ ${stats.shots_on_target[k]}), xG ${stats.xg[k]!.toFixed(1)}`,
        );
      }
    }
    if (s.info.mode === "scenario") {
      for (let k = 0; k < 2; k++) {
        const sh = teamShape(s.snap, s.info, k);
        const f1 = (v: number) => (Number.isFinite(v) ? v.toFixed(0) : "–");
        lines.push(
          `${"AB"[k]} ${s.info.styles[k]} ${s.info.formations[k]}: ${s.snap.teams[k]!.attackW > 0.5 ? "атака" : "оборона"}, ` +
            `в обороне активно ${pressers(s.snap, s.info, k)}, ` +
            `линия ${f1(sh.back)} м, защита–полузащита ${f1(sh.gapBackMid)} м, полузащита–атака ${f1(sh.gapMidFront)} м, ширина ${f1(sh.width)} м`,
        );
      }
    }
    if (lastError) lines.push(`Ошибка: ${lastError}`);
    hud.textContent = lines.join("\n");
    caption.textContent = s.info.scenario
      ? `${s.info.scenario}: ${scenarioDescription(s.info.scenario)}`
      : s.info.mode === "match"
        ? "Матч: мяч по физике, решения владельца, передачи, отбор, удары. Кольцо: жёлтое у владельца, голубое у пасующего."
        : "Песочница физики";
  } else {
    hud.textContent = lastError ? `Ошибка: ${lastError}` : pendingStart ? "Запуск матча…" : "Загрузка движка…";
    const ctx = canvas.getContext("2d");
    if (ctx) {
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.fillStyle = "#2e6b38";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
    }
  }
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);

/** Игроки команды в прессинге, поддержке, страховке или контрпрессинге. */
function pressers(snap: Snapshot, info: MatchInfo, team: number): number {
  let n = 0;
  info.players.forEach((m, i) => {
    if (m.team === team && [2, 3, 4, 6].includes(snap.players[i]!.duty)) n++;
  });
  return n;
}

function fmtTime(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

if (import.meta.env.DEV) {
  (window as unknown as { __fsim: unknown }).__fsim = { get session() { return session; }, ui };
}
