import { describe, expect, it } from "vitest";
import { emptySnapshot, FrameStore, hermite, lerpAngle } from "./frames";
import { Playback } from "./playback";
import type { FrameLayout } from "./generated/FrameLayout";

const HZ = 20;
// Раскладка как у движка, но с двумя игроками, чтобы тесты были короче.
const layout: FrameLayout = {
  stride: 1 + 2 * 12 + 2 * 4 + 8,
  players: 2,
  player_fields: 12,
  team_fields: 4,
  ball_fields: 8,
  players_offset: 1,
  teams_offset: 1 + 2 * 12,
  ball_offset: 1 + 2 * 12 + 2 * 4,
};

interface P { x: number; y: number; vx: number; vy: number; facing?: number; ax?: number; ay?: number; duty?: number; ref?: number }
function frame(tick: number, a: P, b: P, ball: { x: number; y: number; z: number; vx: number; vy: number; vz: number; mode?: number; actor?: number } = { x: 0, y: 0, z: 0, vx: 0, vy: 0, vz: 0 }): Float32Array {
  const f = new Float32Array(layout.stride);
  f[0] = tick;
  [a, b].forEach((p, i) => {
    const o = 1 + i * 12;
    f.set([p.x, p.y, p.vx, p.vy, p.facing ?? 0, 0, 0, 0, p.ax ?? p.x, p.ay ?? p.y, p.duty ?? 0, p.ref ?? 255], o);
  });
  // Команда 0 владеет, линия обороны и офсайда заданы по номеру кадра.
  f.set([1, 10 + tick, 12 + tick, 1], layout.teams_offset);
  f.set([0, -10 - tick, -12 - tick, 0], layout.teams_offset + 4);
  f.set([ball.x, ball.y, ball.z, ball.vx, ball.vy, ball.vz, ball.mode ?? 0, ball.actor ?? 255], layout.ball_offset);
  return f;
}

function concat(frames: Float32Array[]): Float32Array {
  const out = new Float32Array(frames.length * layout.stride);
  frames.forEach((f, i) => out.set(f, i * layout.stride));
  return out;
}

describe("hermite", () => {
  it("совпадает с равномерным движением", () => {
    // x = 3 t + 1 на отрезке 0,05 с.
    const dt = 1 / HZ;
    for (const u of [0, 0.25, 0.5, 1]) {
      expect(hermite(1, 3, 1 + 3 * dt, 3, u, dt)).toBeCloseTo(1 + 3 * dt * u, 6);
    }
  });
  it("воспроизводит параболу точно", () => {
    // x = 0.5 a t^2 при a = 4 (кубика достаточно для квадратичной функции).
    const dt = 1 / HZ;
    const x = (t: number) => 2 * t * t;
    for (const u of [0.1, 0.5, 0.9]) {
      expect(hermite(x(0), 0, x(dt), 4 * dt, u, dt)).toBeCloseTo(x(u * dt), 6);
    }
  });
});

describe("lerpAngle", () => {
  it("идёт по кратчайшей дуге через ±π", () => {
    const a = Math.PI - 0.1;
    const b = -Math.PI + 0.1;
    const mid = lerpAngle(a, b, 0.5);
    expect(Math.abs(Math.abs(mid) - Math.PI)).toBeLessThan(1e-6);
  });
});

describe("FrameStore", () => {
  const still = { x: 0, y: 0, vx: 0, vy: 0 };
  function store(n: number, speed = 5): FrameStore {
    const s = new FrameStore(layout, HZ);
    const frames = [];
    for (let t = 0; t < n; t++) {
      const x = (speed * t) / HZ;
      frames.push(frame(t, { x, y: 0, vx: speed, vy: 0 }, still));
    }
    s.push(concat(frames));
    return s;
  }

  it("запоминает кадры и последний тик", () => {
    const s = store(10);
    expect(s.length).toBe(10);
    expect(s.latestTick()).toBe(9);
  });

  it("игнорирует дубликаты и откаты", () => {
    const s = store(5);
    s.push(concat([frame(3, still, still), frame(4, still, still), frame(5, { ...still, x: 1 }, still)]));
    expect(s.length).toBe(6);
    expect(s.latestTick()).toBe(5);
  });

  it("интерполирует между кадрами без дрожания: траектория гладкая", () => {
    const s = store(30, 6);
    const snap = emptySnapshot(2);
    let prev = -Infinity;
    let maxDev = 0;
    for (let t = 2; t < 20; t += 0.1) {
      s.sample(t, snap);
      expect(snap.players[0]!.x).toBeGreaterThan(prev); // строго вперёд
      prev = snap.players[0]!.x;
      maxDev = Math.max(maxDev, Math.abs(snap.players[0]!.x - (6 * t) / HZ));
    }
    expect(maxDev).toBeLessThan(1e-4);
  });

  it("не выходит за доступный диапазон", () => {
    const s = store(10);
    const snap = emptySnapshot(2);
    s.sample(1000, snap);
    expect(snap.players[0]!.x).toBeCloseTo((5 * 9) / HZ, 5);
    s.sample(-5, snap);
    expect(snap.players[0]!.x).toBeCloseTo(0, 5);
  });

  it("мяч при отскоке не уходит под землю", () => {
    const s = new FrameStore(layout, HZ);
    s.push(concat([
      frame(0, still, still, { x: 0, y: 0, z: 0.11, vx: 0, vy: 0, vz: -6 }),
      frame(1, still, still, { x: 0, y: 0, z: 0.11, vx: 0, vy: 0, vz: 4 }),
    ]));
    const snap = emptySnapshot(2);
    for (let t = 0; t <= 1; t += 0.05) {
      s.sample(t, snap);
      expect(snap.ball.z).toBeGreaterThanOrEqual(0);
    }
  });

  it("чистит старые кадры, но оставляет два для интерполяции", () => {
    const s = store(200);
    s.prune(199, 50);
    expect(s.firstTick()).toBeGreaterThanOrEqual(148);
    expect(s.length).toBeGreaterThanOrEqual(2);
    const snap = emptySnapshot(2);
    expect(s.sample(199, snap)).toBe(true);
  });

  it("отдаёт следы игрока за окно", () => {
    const s = store(100);
    const out: number[] = [];
    s.trail(0, 80, 20, out);
    expect(out.length / 2).toBe(21);
    expect(out[0]).toBeCloseTo((5 * 60) / HZ, 5);
  });
});

describe("команды и якоря", () => {
  it("интерполирует линию обороны и отдаёт якоря", () => {
    const s = new FrameStore(layout, HZ);
    const a = { x: 0, y: 0, vx: 0, vy: 0, ax: 3, ay: 4 };
    s.push(concat([frame(0, a, a), frame(2, a, a)]));
    const snap = emptySnapshot(2);
    s.sample(1, snap);
    expect(snap.teams[0]!.lineX).toBeCloseTo(11, 5); // между 10 и 12
    expect(snap.teams[0]!.offsideX).toBeCloseTo(13, 5);
    expect(snap.teams[0]!.hasBall).toBe(true);
    expect(snap.teams[1]!.hasBall).toBe(false);
    expect(snap.teams[1]!.lineX).toBeCloseTo(-11, 5);
    expect([snap.players[0]!.anchorX, snap.players[0]!.anchorY]).toEqual([3, 4]);
  });

  it("отдаёт обязанность и ссылку на игрока без интерполяции", () => {
    const s = new FrameStore(layout, HZ);
    const a = { x: 0, y: 0, vx: 0, vy: 0, duty: 2, ref: 1 };
    const b = { x: 1, y: 0, vx: 0, vy: 0, duty: 4, ref: 255 };
    s.push(concat([frame(0, a, b), frame(2, a, b)]));
    const snap = emptySnapshot(2);
    s.sample(1, snap);
    expect(snap.players[0]!.duty).toBe(2);
    expect(snap.players[0]!.dutyRef).toBe(1);
    expect(snap.players[1]!.duty).toBe(4);
    expect(snap.players[1]!.dutyRef).toBe(255);
  });
});

describe("режим мяча", () => {
  it("отдаёт режим и игрока без интерполяции", () => {
    const s = new FrameStore(layout, HZ);
    const z = { x: 0, y: 0, vx: 0, vy: 0 };
    s.push(concat([
      frame(0, z, z, { x: 0, y: 0, z: 0.1, vx: 0, vy: 0, vz: 0, mode: 1, actor: 3 }),
      frame(2, z, z, { x: 1, y: 0, z: 0.1, vx: 0, vy: 0, vz: 0, mode: 2, actor: 3 }),
    ]));
    const snap = emptySnapshot(2);
    s.sample(1, snap);
    expect(snap.ball.mode).toBe(2);
    expect(snap.ball.actor).toBe(3);
  });
});

describe("Playback", () => {
  it("идёт со скоростью × тики в секунду и упирается в готовые кадры", () => {
    const p = new Playback(HZ);
    p.speed = 4;
    p.advance(0.5, 1000);
    expect(p.tick).toBeCloseTo(40, 6);
    p.advance(10, 55);
    expect(p.tick).toBe(55);
  });
  it("на паузе стоит, шаг двигает на один тик", () => {
    const p = new Playback(HZ);
    p.paused = true;
    p.advance(5, 1000);
    expect(p.tick).toBe(0);
    p.stepOnce(1000);
    expect(p.tick).toBe(1);
    expect(p.paused).toBe(true);
    p.stepOnce(1000);
    expect(p.tick).toBe(2);
  });
  it("запас кадров растёт со скоростью", () => {
    const p = new Playback(HZ);
    p.speed = 1;
    const a = p.leadTicks();
    p.speed = 16;
    expect(p.leadTicks()).toBeGreaterThan(a);
  });
});
