// Хранилище кадров физики и интерполяция между ними.
import type { FrameLayout } from "./generated/FrameLayout";

/** Состояние игрока в момент отрисовки. */
export interface PlayerState {
  x: number;
  y: number;
  vx: number;
  vy: number;
  facing: number;
  targetX: number;
  targetY: number;
  speedCap: number;
  anchorX: number;
  anchorY: number;
  /** Обязанность: 0 нет, 1 зона, 2 прессинг, 3 тень, 4 страховка, 5 опека, 6 контрпрессинг, 7 владелец. */
  duty: number;
  /** Игрок, к которому относится обязанность (общий индекс), или 255. */
  dutyRef: number;
}

/** Состояние команды: фаза, линия обороны и линия офсайда (мировая x). */
export interface TeamSnap {
  attackW: number;
  lineX: number;
  offsideX: number;
  hasBall: boolean;
}

export interface BallState {
  x: number;
  y: number;
  z: number;
  vx: number;
  vy: number;
  vz: number;
  /** Режим мяча: 0 по скрипту, 1 у ног, 2 в полёте, 3 ничей, 4 вне игры. */
  mode: number;
  /** Игрок, к которому относится режим (общий индекс), или 255. */
  actor: number;
}

export interface Snapshot {
  players: PlayerState[];
  teams: TeamSnap[];
  ball: BallState;
}

export function emptySnapshot(players: number): Snapshot {
  return {
    players: Array.from({ length: players }, () => ({
      x: 0, y: 0, vx: 0, vy: 0, facing: 0, targetX: 0, targetY: 0, speedCap: 0, anchorX: 0, anchorY: 0, duty: 0, dutyRef: 255,
    })),
    teams: [0, 1].map(() => ({ attackW: 0, lineX: 0, offsideX: 0, hasBall: false })),
    ball: { x: 0, y: 0, z: 0, vx: 0, vy: 0, vz: 0, mode: 0, actor: 255 },
  };
}

/** Кубическая интерполяция Эрмита по положению и скорости: гладкая, без изломов на стыках кадров. */
export function hermite(p0: number, v0: number, p1: number, v1: number, t: number, dt: number): number {
  const t2 = t * t;
  const t3 = t2 * t;
  return (
    (2 * t3 - 3 * t2 + 1) * p0 +
    (t3 - 2 * t2 + t) * dt * v0 +
    (-2 * t3 + 3 * t2) * p1 +
    (t3 - t2) * dt * v1
  );
}

/** Интерполяция угла по кратчайшей дуге. */
export function lerpAngle(a: number, b: number, t: number): number {
  let d = (b - a) % (2 * Math.PI);
  if (d > Math.PI) d -= 2 * Math.PI;
  if (d < -Math.PI) d += 2 * Math.PI;
  return a + d * t;
}

/** Кадры матча по возрастанию тика. */
export class FrameStore {
  private frames: Float32Array[] = [];
  readonly stride: number;

  constructor(
    private readonly layout: FrameLayout,
    private readonly tickHz: number,
  ) {
    this.stride = layout.stride;
  }

  /** Добавляет пачку кадров из воркера (подряд по `stride` чисел). */
  push(buffer: Float32Array): void {
    for (let o = 0; o + this.stride <= buffer.length; o += this.stride) {
      const f = buffer.subarray(o, o + this.stride);
      const last = this.frames[this.frames.length - 1];
      if (last && f[0]! <= last[0]!) continue; // дубликаты и откаты игнорируем
      this.frames.push(f);
    }
  }

  get length(): number {
    return this.frames.length;
  }

  firstTick(): number {
    return this.frames[0]?.[0] ?? 0;
  }

  latestTick(): number {
    return this.frames[this.frames.length - 1]?.[0] ?? -1;
  }

  /** Забывает кадры старше `keepTicks` от тика `now`, чтобы память не росла. */
  prune(now: number, keepTicks: number): void {
    let drop = 0;
    while (drop < this.frames.length - 2 && this.frames[drop + 1]![0]! < now - keepTicks) drop++;
    if (drop > 0) this.frames.splice(0, drop);
  }

  /** Индекс последнего кадра с тиком не больше `tick` или -1. */
  private indexAtOrBefore(tick: number): number {
    let lo = 0;
    let hi = this.frames.length - 1;
    let ans = -1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (this.frames[mid]![0]! <= tick) {
        ans = mid;
        lo = mid + 1;
      } else hi = mid - 1;
    }
    return ans;
  }

  /** Состояние в дробный тик `tick`; ограничивается доступным диапазоном. Возвращает false, если кадров нет. */
  sample(tick: number, out: Snapshot): boolean {
    if (this.frames.length === 0) return false;
    const t = Math.min(Math.max(tick, this.firstTick()), this.latestTick());
    const i = this.indexAtOrBefore(t);
    const a = this.frames[i]!;
    const b = this.frames[Math.min(i + 1, this.frames.length - 1)]!;
    const span = b[0]! - a[0]!;
    const u = span > 0 ? (t - a[0]!) / span : 0;
    const dt = span / this.tickHz;
    const l = this.layout;
    for (let k = 0; k < l.players; k++) {
      const o = l.players_offset + k * l.player_fields;
      const p = out.players[k]!;
      p.x = hermite(a[o]!, a[o + 2]!, b[o]!, b[o + 2]!, u, dt);
      p.y = hermite(a[o + 1]!, a[o + 3]!, b[o + 1]!, b[o + 3]!, u, dt);
      p.vx = a[o + 2]! + (b[o + 2]! - a[o + 2]!) * u;
      p.vy = a[o + 3]! + (b[o + 3]! - a[o + 3]!) * u;
      p.facing = lerpAngle(a[o + 4]!, b[o + 4]!, u);
      p.targetX = b[o + 5]!;
      p.targetY = b[o + 6]!;
      p.speedCap = b[o + 7]!;
      p.anchorX = b[o + 8]!;
      p.anchorY = b[o + 9]!;
      p.duty = b[o + 10]!;
      p.dutyRef = b[o + 11]!;
    }
    for (let k = 0; k < 2; k++) {
      const o = l.teams_offset + k * l.team_fields;
      const t = out.teams[k]!;
      t.attackW = a[o]! + (b[o]! - a[o]!) * u;
      t.lineX = a[o + 1]! + (b[o + 1]! - a[o + 1]!) * u;
      t.offsideX = a[o + 2]! + (b[o + 2]! - a[o + 2]!) * u;
      t.hasBall = b[o + 3]! > 0.5;
    }
    const o = l.ball_offset;
    const ball = out.ball;
    ball.x = hermite(a[o]!, a[o + 3]!, b[o]!, b[o + 3]!, u, dt);
    ball.y = hermite(a[o + 1]!, a[o + 4]!, b[o + 1]!, b[o + 4]!, u, dt);
    // Высота не опускается ниже нуля из-за перерегулирования интерполяции.
    ball.z = Math.max(0, hermite(a[o + 2]!, a[o + 5]!, b[o + 2]!, b[o + 5]!, u, dt));
    ball.vx = a[o + 3]! + (b[o + 3]! - a[o + 3]!) * u;
    ball.vy = a[o + 4]! + (b[o + 4]! - a[o + 4]!) * u;
    ball.vz = a[o + 5]! + (b[o + 5]! - a[o + 5]!) * u;
    ball.mode = b[o + 6]!;
    ball.actor = b[o + 7]!;
    return true;
  }

  /** Позиции игрока за последние `ticks` тиков до `now` для следов: плоский массив x, y. */
  trail(player: number, now: number, ticks: number, out: number[]): void {
    out.length = 0;
    const l = this.layout;
    const o = l.players_offset + player * l.player_fields;
    const from = this.indexAtOrBefore(now - ticks);
    const to = this.indexAtOrBefore(now);
    if (to < 0) return;
    for (let i = Math.max(from, 0); i <= to; i++) {
      const f = this.frames[i]!;
      out.push(f[o]!, f[o + 1]!);
    }
  }
}
