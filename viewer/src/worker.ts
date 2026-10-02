// Воркер: движок на WASM считает матч и отдаёт кадры transferable-буфером.
import init, { catalog, Engine } from "./wasm/fsim_wasm.js";
import type { FromWorker, ToWorker } from "./protocol";

const ctx = self as unknown as {
  postMessage(m: FromWorker, t?: Transferable[]): void;
  onmessage: ((e: MessageEvent<ToWorker>) => void) | null;
};

let engine: Engine | null = null;
let gen = -1;
const ready = init().then(() => ctx.postMessage({ type: "init", catalog: JSON.parse(catalog()) }));

ctx.onmessage = async (e) => {
  await ready;
  const m = e.data;
  try {
    if (m.type === "new") {
      engine?.free();
      gen = m.gen;
      const seed = BigInt(m.seed);
      engine =
        m.mode === "scenario"
          ? Engine.scenario(seed, m.scenario, m.styleA, m.styleB, m.formationA, m.formationB, m.quality)
          : m.mode === "match"
            ? Engine.live(seed, m.quality, m.styleA, m.styleB, m.formationA, m.formationB)
            : new Engine(seed, m.quality, m.formationA || "4-3-3", m.formationB || "4-4-2");
      ctx.postMessage({ type: "ready", gen, info: JSON.parse(engine.info()) });
      sendFrames(engine.run_to(0), engine.stats());
    } else if (m.type === "run_to" && engine && m.gen === gen) {
      const frames = engine.run_to(m.tick);
      if (frames.length > 0) sendFrames(frames, engine.stats());
    }
  } catch (err) {
    ctx.postMessage({ type: "error", gen, message: String(err) });
  }
};

function sendFrames(frames: Float32Array, stats: string) {
  ctx.postMessage({ type: "frames", gen, buffer: frames.buffer as ArrayBuffer, stats }, [frames.buffer as ArrayBuffer]);
}
