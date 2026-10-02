// Отрисовка поля, игроков и мяча на Canvas 2D. Масштаб честный: кружок игрока около 0,8 м.
import type { MatchInfo } from "./generated/MatchInfo";
import type { Camera } from "./camera";
import type { FrameStore, Snapshot } from "./frames";
import { teamShape } from "./shape";

export interface RenderOptions {
  trails: boolean;
  targets: boolean;
  numbers: boolean;
  anchors: boolean;
  lines: boolean;
  hull: boolean;
  duties: boolean;
}

/** Визуальные константы: размеры разметки по правилам и палитра. Физики они не касаются. */
const PLAYER_DRAW_RADIUS_M = 0.8;
const BALL_DRAW_RADIUS_M = 0.35;
const TRAIL_SECONDS = 2.5;
const LINE_M = 0.14;
const PENALTY_AREA = { depth: 16.5, width: 40.32 };
const GOAL_AREA = { depth: 5.5, width: 18.32 };
const PENALTY_SPOT_M = 11;
const CENTER_CIRCLE_R = 9.15;
const CORNER_R = 1;
const STRIPES = 12;

/** Обязанности по `defense::DUTY_*`: буква значка и цвет. Зона и вратарь без значка. */
const DUTIES: Record<number, { letter: string; color: string }> = {
  2: { letter: "П", color: "#ffd23f" }, // прессинг
  3: { letter: "Т", color: "#7be0ff" }, // тень прикрытия
  4: { letter: "С", color: "#c4a1ff" }, // страховка
  5: { letter: "О", color: "#ff9f5a" }, // опека
  6: { letter: "К", color: "#ff6b8b" }, // контрпрессинг
  7: { letter: "М", color: "#ffffff" }, // владелец мяча
};

const PALETTE = {
  grassA: "#3f8a4a",
  grassB: "#388243",
  line: "rgba(255,255,255,0.85)",
  net: "rgba(255,255,255,0.55)",
  teams: ["#2f6fed", "#e6533c"],
  keepers: ["#f2c230", "#7ad1c0"],
  outline: "#ffffff",
  number: "#ffffff",
  numberShadow: "rgba(0,0,0,0.65)",
  ball: "#fafafa",
  ballEdge: "#222",
  shadow: "rgba(0,0,0,0.35)",
  target: "rgba(255,255,255,0.6)",
};

export class Renderer {
  private readonly ctx: CanvasRenderingContext2D;
  private trail: number[] = [];

  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly info: MatchInfo,
  ) {
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Canvas 2D недоступен");
    this.ctx = ctx;
  }

  draw(snap: Snapshot, cam: Camera, store: FrameStore, nowTick: number, opts: RenderOptions): void {
    const { ctx } = this;
    const dpr = window.devicePixelRatio || 1;
    const w = this.canvas.width / dpr;
    const h = this.canvas.height / dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = "#2e6b38";
    ctx.fillRect(0, 0, w, h);
    this.drawPitch(cam, w, h);
    if (opts.hull) this.drawHulls(snap, cam, w, h);
    if (opts.lines) this.drawLines(snap, cam, w, h);
    if (opts.anchors) this.drawAnchors(snap, cam, w, h);
    if (opts.duties) this.drawDuties(snap, cam, w, h);
    if (opts.trails) this.drawTrails(cam, store, nowTick, w, h);
    if (opts.targets) this.drawTargets(snap, cam, w, h);
    this.drawBallShadow(snap, cam, w, h);
    this.drawPlayers(snap, cam, w, h, opts);
    this.drawBallHolder(snap, cam, w, h);
    this.drawBall(snap, cam, w, h);
  }

  private drawPitch(cam: Camera, w: number, h: number): void {
    const { ctx } = this;
    const L = this.info.pitch.length_m;
    const W = this.info.pitch.width_m;
    const s = cam.scale;
    const [x0, y0] = cam.toScreen(-L / 2, -W / 2, w, h);
    // Полосы покоса.
    const stripe = (L * s) / STRIPES;
    for (let i = 0; i < STRIPES; i++) {
      ctx.fillStyle = i % 2 === 0 ? PALETTE.grassA : PALETTE.grassB;
      ctx.fillRect(x0 + i * stripe, y0, stripe + 0.5, W * s);
    }
    ctx.strokeStyle = PALETTE.line;
    ctx.lineWidth = Math.max(1, LINE_M * s);
    ctx.lineJoin = "round";
    const rect = (ax: number, ay: number, bx: number, by: number) => {
      const [sx, sy] = cam.toScreen(ax, ay, w, h);
      ctx.strokeRect(sx, sy, (bx - ax) * s, (by - ay) * s);
    };
    rect(-L / 2, -W / 2, L / 2, W / 2);
    // Центр.
    const line = (ax: number, ay: number, bx: number, by: number) => {
      const [sx, sy] = cam.toScreen(ax, ay, w, h);
      const [ex, ey] = cam.toScreen(bx, by, w, h);
      ctx.beginPath();
      ctx.moveTo(sx, sy);
      ctx.lineTo(ex, ey);
      ctx.stroke();
    };
    const arc = (cx: number, cy: number, r: number, a0: number, a1: number) => {
      const [sx, sy] = cam.toScreen(cx, cy, w, h);
      ctx.beginPath();
      ctx.arc(sx, sy, r * s, a0, a1);
      ctx.stroke();
    };
    const dot = (cx: number, cy: number) => {
      const [sx, sy] = cam.toScreen(cx, cy, w, h);
      ctx.fillStyle = PALETTE.line;
      ctx.beginPath();
      ctx.arc(sx, sy, Math.max(1.5, 0.25 * s), 0, Math.PI * 2);
      ctx.fill();
    };
    line(0, -W / 2, 0, W / 2);
    arc(0, 0, CENTER_CIRCLE_R, 0, Math.PI * 2);
    dot(0, 0);
    for (const side of [-1, 1]) {
      const gx = side * (L / 2);
      // Штрафная и вратарская площади тянутся от линии ворот внутрь поля.
      rect(Math.min(gx, gx - side * PENALTY_AREA.depth), -PENALTY_AREA.width / 2,
           Math.max(gx, gx - side * PENALTY_AREA.depth), PENALTY_AREA.width / 2);
      rect(Math.min(gx, gx - side * GOAL_AREA.depth), -GOAL_AREA.width / 2,
           Math.max(gx, gx - side * GOAL_AREA.depth), GOAL_AREA.width / 2);
      const spotX = gx - side * PENALTY_SPOT_M;
      dot(spotX, 0);
      // Дуга штрафной вне площади.
      const cut = Math.acos((PENALTY_AREA.depth - PENALTY_SPOT_M) / CENTER_CIRCLE_R);
      if (side === 1) arc(spotX, 0, CENTER_CIRCLE_R, Math.PI - cut, Math.PI + cut);
      else arc(spotX, 0, CENTER_CIRCLE_R, -cut, cut);
      this.drawGoal(cam, w, h, gx, side);
    }
    // Угловые дуги.
    arc(-L / 2, -W / 2, CORNER_R, 0, Math.PI / 2);
    arc(-L / 2, W / 2, CORNER_R, -Math.PI / 2, 0);
    arc(L / 2, -W / 2, CORNER_R, Math.PI / 2, Math.PI);
    arc(L / 2, W / 2, CORNER_R, Math.PI, Math.PI * 1.5);
  }

  private drawGoal(cam: Camera, w: number, h: number, gx: number, side: number): void {
    const { ctx } = this;
    const { goal_width_m: gw, goal_depth_m: gd } = this.info.pitch;
    const s = cam.scale;
    const [sx, sy] = cam.toScreen(Math.min(gx, gx + side * gd), -gw / 2, w, h);
    ctx.save();
    ctx.fillStyle = "rgba(255,255,255,0.12)";
    ctx.fillRect(sx, sy, gd * s, gw * s);
    ctx.strokeStyle = PALETTE.net;
    ctx.lineWidth = Math.max(1, 0.08 * s);
    ctx.strokeRect(sx, sy, gd * s, gw * s);
    // Сетка: редкие линии, чтобы читалась как сетка и не шумела.
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let i = 1; i < 6; i++) {
      ctx.moveTo(sx, sy + (gw * s * i) / 6);
      ctx.lineTo(sx + gd * s, sy + (gw * s * i) / 6);
    }
    for (let i = 1; i < 4; i++) {
      ctx.moveTo(sx + (gd * s * i) / 4, sy);
      ctx.lineTo(sx + (gd * s * i) / 4, sy + gw * s);
    }
    ctx.stroke();
    ctx.restore();
    // Штанги.
    ctx.fillStyle = "#fff";
    for (const y of [-gw / 2, gw / 2]) {
      const [px, py] = cam.toScreen(gx, y, w, h);
      ctx.beginPath();
      ctx.arc(px, py, Math.max(2, 0.14 * s), 0, Math.PI * 2);
      ctx.fill();
    }
  }

  /** Контур формы: выпуклая оболочка полевых игроков каждой команды. */
  private drawHulls(snap: Snapshot, cam: Camera, w: number, h: number): void {
    const { ctx } = this;
    for (let team = 0; team < 2; team++) {
      const shape = teamShape(snap, this.info, team);
      if (shape.hull.length < 3) continue;
      ctx.beginPath();
      shape.hull.forEach(([x, y], i) => {
        const [sx, sy] = cam.toScreen(x, y, w, h);
        if (i === 0) ctx.moveTo(sx, sy);
        else ctx.lineTo(sx, sy);
      });
      ctx.closePath();
      ctx.fillStyle = PALETTE.teams[team]!;
      ctx.globalAlpha = 0.14;
      ctx.fill();
      ctx.globalAlpha = 0.7;
      ctx.strokeStyle = PALETTE.teams[team]!;
      ctx.lineWidth = Math.max(1.5, 0.2 * cam.scale);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }
  }

  /** Линия обороны (сплошная) и линия офсайда (штриховая) каждой команды. */
  private drawLines(snap: Snapshot, cam: Camera, w: number, h: number): void {
    const { ctx } = this;
    const half = this.info.pitch.width_m / 2;
    ctx.lineWidth = Math.max(1.5, 0.22 * cam.scale);
    for (let team = 0; team < 2; team++) {
      const t = snap.teams[team]!;
      ctx.strokeStyle = PALETTE.teams[team]!;
      for (const [x, dashed] of [[t.lineX, false], [t.offsideX, true]] as const) {
        const [ax, ay] = cam.toScreen(x, -half, w, h);
        const [bx, by] = cam.toScreen(x, half, w, h);
        ctx.setLineDash(dashed ? [8, 6] : []);
        ctx.globalAlpha = dashed ? 0.55 : 0.9;
        ctx.beginPath();
        ctx.moveTo(ax, ay);
        ctx.lineTo(bx, by);
        ctx.stroke();
      }
    }
    ctx.setLineDash([]);
    ctx.globalAlpha = 1;
  }

  /** Обязанности в обороне: линия от игрока к его цели и значок вида обязанности. */
  private drawDuties(snap: Snapshot, cam: Camera, w: number, h: number): void {
    const { ctx } = this;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.font = `700 ${Math.max(9, cam.scale * 0.9)}px system-ui, sans-serif`;
    for (let i = 0; i < snap.players.length; i++) {
      const p = snap.players[i]!;
      const style = DUTIES[p.duty];
      if (!style) continue;
      const [px, py] = cam.toScreen(p.x, p.y, w, h);
      if (p.dutyRef !== 255 && snap.players[p.dutyRef]) {
        const o = snap.players[p.dutyRef]!;
        const [ox, oy] = cam.toScreen(o.x, o.y, w, h);
        ctx.strokeStyle = style.color;
        ctx.lineWidth = Math.max(1.5, 0.18 * cam.scale);
        ctx.setLineDash(p.duty === 5 ? [4, 3] : []);
        ctx.globalAlpha = 0.85;
        ctx.beginPath();
        ctx.moveTo(px, py);
        ctx.lineTo(ox, oy);
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.globalAlpha = 1;
      }
      // Значок над игроком.
      const r = Math.max(6, cam.scale * 0.75);
      const bx = px + PLAYER_DRAW_RADIUS_M * cam.scale * 0.9;
      const by = py - PLAYER_DRAW_RADIUS_M * cam.scale * 0.9;
      ctx.fillStyle = style.color;
      ctx.beginPath();
      ctx.arc(bx, by, r, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = "#101418";
      ctx.fillText(style.letter, bx, by + 0.5);
    }
  }

  /** Якоря: кольцо в точке и тонкая линия от игрока к ней. */
  private drawAnchors(snap: Snapshot, cam: Camera, w: number, h: number): void {
    const { ctx } = this;
    ctx.lineWidth = 1;
    for (let i = 0; i < snap.players.length; i++) {
      const p = snap.players[i]!;
      const color = PALETTE.teams[this.info.players[i]!.team]!;
      const [px, py] = cam.toScreen(p.x, p.y, w, h);
      const [ax, ay] = cam.toScreen(p.anchorX, p.anchorY, w, h);
      ctx.strokeStyle = color;
      ctx.globalAlpha = 0.5;
      ctx.beginPath();
      ctx.moveTo(px, py);
      ctx.lineTo(ax, ay);
      ctx.stroke();
      ctx.globalAlpha = 0.95;
      ctx.lineWidth = Math.max(1.5, 0.15 * cam.scale);
      ctx.beginPath();
      ctx.arc(ax, ay, Math.max(3, 0.45 * cam.scale), 0, Math.PI * 2);
      ctx.stroke();
      ctx.lineWidth = 1;
    }
    ctx.globalAlpha = 1;
  }

  private drawTrails(cam: Camera, store: FrameStore, nowTick: number, w: number, h: number): void {
    const { ctx } = this;
    const ticks = TRAIL_SECONDS * this.info.tick_hz;
    ctx.lineCap = "round";
    ctx.lineWidth = Math.max(1, 0.18 * cam.scale);
    for (let i = 0; i < this.info.players.length; i++) {
      store.trail(i, nowTick, ticks, this.trail);
      const n = this.trail.length / 2;
      if (n < 2) continue;
      const color = PALETTE.teams[this.info.players[i]!.team]!;
      ctx.strokeStyle = color;
      for (let k = 1; k < n; k++) {
        ctx.globalAlpha = (k / n) * 0.55;
        const [ax, ay] = cam.toScreen(this.trail[2 * k - 2]!, this.trail[2 * k - 1]!, w, h);
        const [bx, by] = cam.toScreen(this.trail[2 * k]!, this.trail[2 * k + 1]!, w, h);
        ctx.beginPath();
        ctx.moveTo(ax, ay);
        ctx.lineTo(bx, by);
        ctx.stroke();
      }
    }
    ctx.globalAlpha = 1;
  }

  private drawTargets(snap: Snapshot, cam: Camera, w: number, h: number): void {
    const { ctx } = this;
    ctx.strokeStyle = PALETTE.target;
    ctx.lineWidth = 1;
    ctx.setLineDash([4, 4]);
    const cross = Math.max(3, 0.4 * cam.scale);
    for (const p of snap.players) {
      const [ax, ay] = cam.toScreen(p.x, p.y, w, h);
      const [bx, by] = cam.toScreen(p.targetX, p.targetY, w, h);
      ctx.beginPath();
      ctx.moveTo(ax, ay);
      ctx.lineTo(bx, by);
      ctx.stroke();
    }
    ctx.setLineDash([]);
    for (const p of snap.players) {
      const [bx, by] = cam.toScreen(p.targetX, p.targetY, w, h);
      ctx.beginPath();
      ctx.moveTo(bx - cross, by - cross);
      ctx.lineTo(bx + cross, by + cross);
      ctx.moveTo(bx - cross, by + cross);
      ctx.lineTo(bx + cross, by - cross);
      ctx.stroke();
    }
  }

  private drawPlayers(snap: Snapshot, cam: Camera, w: number, h: number, opts: RenderOptions): void {
    const { ctx } = this;
    const r = PLAYER_DRAW_RADIUS_M * cam.scale;
    const walk = this.info.gait_walk_fraction;
    const jog = this.info.gait_jog_fraction;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.font = `600 ${Math.max(8, r * 1.05)}px system-ui, sans-serif`;
    for (let i = 0; i < snap.players.length; i++) {
      const p = snap.players[i]!;
      const meta = this.info.players[i]!;
      const [sx, sy] = cam.toScreen(p.x, p.y, w, h);
      const keeper = meta.number === 1;
      ctx.fillStyle = (keeper ? PALETTE.keepers : PALETTE.teams)[meta.team]!;
      ctx.beginPath();
      ctx.arc(sx, sy, r, 0, Math.PI * 2);
      ctx.fill();
      // Интенсивность бега: толщина обводки (шаг, бег, спринт); на месте самая тонкая.
      const speed = Math.hypot(p.vx, p.vy);
      const frac = meta.max_speed_ms > 0 ? p.speedCap / meta.max_speed_ms : 0;
      const weight = speed < 0.3 ? 0.06 : frac <= walk + 0.01 ? 0.1 : frac <= jog + 0.01 ? 0.2 : 0.36;
      ctx.strokeStyle = PALETTE.outline;
      ctx.lineWidth = Math.max(1, r * weight);
      ctx.stroke();
      // Направление взгляда.
      ctx.strokeStyle = PALETTE.outline;
      ctx.lineWidth = Math.max(1.5, r * 0.18);
      ctx.beginPath();
      ctx.moveTo(sx + Math.cos(p.facing) * r * 0.55, sy + Math.sin(p.facing) * r * 0.55);
      ctx.lineTo(sx + Math.cos(p.facing) * r * 1.5, sy + Math.sin(p.facing) * r * 1.5);
      ctx.stroke();
      if (opts.numbers) {
        ctx.fillStyle = PALETTE.numberShadow;
        ctx.fillText(String(meta.number), sx + 0.5, sy + 1);
        ctx.fillStyle = PALETTE.number;
        ctx.fillText(String(meta.number), sx, sy);
      }
    }
  }

  /** Кольцо вокруг игрока, у которого мяч (владелец, пасующий или исполнитель розыгрыша). */
  private drawBallHolder(snap: Snapshot, cam: Camera, w: number, h: number): void {
    const { ctx } = this;
    const a = snap.ball.actor;
    const p = snap.players[a];
    if (!p || snap.ball.mode === 0 || snap.ball.mode === 3) return;
    const [sx, sy] = cam.toScreen(p.x, p.y, w, h);
    ctx.strokeStyle = snap.ball.mode === 1 ? "#ffd23f" : snap.ball.mode === 2 ? "#7be0ff" : "#ffffff";
    ctx.lineWidth = Math.max(2, 0.2 * cam.scale);
    ctx.beginPath();
    ctx.arc(sx, sy, PLAYER_DRAW_RADIUS_M * cam.scale * 1.6, 0, Math.PI * 2);
    ctx.stroke();
  }

  /** Тень остаётся на земле, мяч уходит от неё по высоте: при навесе тень «отстаёт». */
  private drawBallShadow(snap: Snapshot, cam: Camera, w: number, h: number): void {
    const { ctx } = this;
    const b = snap.ball;
    const [sx, sy] = cam.toScreen(b.x, b.y, w, h);
    ctx.fillStyle = PALETTE.shadow;
    ctx.beginPath();
    ctx.ellipse(sx, sy, BALL_DRAW_RADIUS_M * cam.scale * 1.05, BALL_DRAW_RADIUS_M * cam.scale * 0.8, 0, 0, Math.PI * 2);
    ctx.fill();
  }

  private drawBall(snap: Snapshot, cam: Camera, w: number, h: number): void {
    const { ctx } = this;
    const b = snap.ball;
    // Подъём на экране пропорционален высоте; размер растёт: ближе к камере.
    const lift = 0.45 * b.z;
    const [sx, sy] = cam.toScreen(b.x, b.y - lift, w, h);
    const r = Math.max(3.5, BALL_DRAW_RADIUS_M * cam.scale * (1 + 0.06 * b.z));
    ctx.fillStyle = PALETTE.ball;
    ctx.strokeStyle = PALETTE.ballEdge;
    ctx.lineWidth = Math.max(1, r * 0.2);
    ctx.beginPath();
    ctx.arc(sx, sy, r, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
  }
}
