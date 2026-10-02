// Метрики формы по текущим позициям: контур блока и расстояния между линиями.
import type { MatchInfo } from "./generated/MatchInfo";
import type { Snapshot } from "./frames";

export type Pt = [number, number];

/** Выпуклая оболочка (алгоритм Эндрю), по часовой стрелке в экранных координатах. */
export function convexHull(points: Pt[]): Pt[] {
  const p = [...points].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  if (p.length < 3) return p;
  const cross = (o: Pt, a: Pt, b: Pt) => (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  const lower: Pt[] = [];
  for (const q of p) {
    while (lower.length >= 2 && cross(lower[lower.length - 2]!, lower[lower.length - 1]!, q) <= 0) lower.pop();
    lower.push(q);
  }
  const upper: Pt[] = [];
  for (let i = p.length - 1; i >= 0; i--) {
    const q = p[i]!;
    while (upper.length >= 2 && cross(upper[upper.length - 2]!, upper[upper.length - 1]!, q) <= 0) upper.pop();
    upper.push(q);
  }
  lower.pop();
  upper.pop();
  return lower.concat(upper);
}

/** Площадь многоугольника. */
export function polygonArea(poly: Pt[]): number {
  let s = 0;
  for (let i = 0; i < poly.length; i++) {
    const a = poly[i]!;
    const b = poly[(i + 1) % poly.length]!;
    s += a[0] * b[1] - b[0] * a[1];
  }
  return Math.abs(s) / 2;
}

export interface TeamShape {
  /** Полевые игроки команды в мировых координатах. */
  outfield: Pt[];
  hull: Pt[];
  /** Средняя глубина линий от своих ворот, м; NaN, если в линии никого нет. */
  back: number;
  mid: number;
  front: number;
  gapBackMid: number;
  gapMidFront: number;
  width: number;
}

/** Расстояние от своей линии ворот вдоль поля. */
function depth(team: number, x: number, halfLen: number): number {
  return team === 0 ? x + halfLen : halfLen - x;
}

export function teamShape(snap: Snapshot, info: MatchInfo, team: number): TeamShape {
  const half = info.pitch.length_m / 2;
  const sums = [0, 0, 0, 0];
  const counts = [0, 0, 0, 0];
  const outfield: Pt[] = [];
  let minY = Infinity;
  let maxY = -Infinity;
  info.players.forEach((meta, i) => {
    if (meta.team !== team || meta.line === 0) return;
    const p = snap.players[i]!;
    outfield.push([p.x, p.y]);
    sums[meta.line]! += depth(team, p.x, half);
    counts[meta.line]!++;
    minY = Math.min(minY, p.y);
    maxY = Math.max(maxY, p.y);
  });
  const mean = (l: number) => (counts[l]! > 0 ? sums[l]! / counts[l]! : NaN);
  const [back, mid, front] = [mean(1), mean(2), mean(3)];
  return {
    outfield,
    hull: convexHull(outfield),
    back,
    mid,
    front,
    gapBackMid: mid - back,
    gapMidFront: front - mid,
    width: maxY - minY,
  };
}
