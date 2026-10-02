import { describe, expect, it } from "vitest";
import { convexHull, polygonArea, teamShape, type Pt } from "./shape";
import { emptySnapshot } from "./frames";
import type { MatchInfo } from "./generated/MatchInfo";

describe("convexHull", () => {
  it("находит оболочку квадрата и отбрасывает внутренние точки", () => {
    const pts: Pt[] = [[0, 0], [4, 0], [4, 4], [0, 4], [2, 2], [1, 3], [3, 1]];
    const h = convexHull(pts);
    expect(h.length).toBe(4);
    expect(polygonArea(h)).toBeCloseTo(16, 6);
  });
  it("отбрасывает точки на ребре", () => {
    const h = convexHull([[0, 0], [2, 0], [4, 0], [4, 4], [0, 4]]);
    expect(h.length).toBe(4);
  });
  it("работает на малом числе точек", () => {
    expect(convexHull([[1, 1]]).length).toBe(1);
    expect(convexHull([[1, 1], [2, 2]]).length).toBe(2);
  });
});

describe("teamShape", () => {
  const info = {
    pitch: { length_m: 100, width_m: 60 },
    players: [
      { team: 0, line: 0 }, { team: 0, line: 1 }, { team: 0, line: 1 },
      { team: 0, line: 2 }, { team: 0, line: 3 },
      { team: 1, line: 0 }, { team: 1, line: 1 },
    ],
  } as unknown as MatchInfo;

  it("считает глубину линий и расстояния между ними от своих ворот", () => {
    const snap = emptySnapshot(7);
    const set = (i: number, x: number, y: number) => { snap.players[i]!.x = x; snap.players[i]!.y = y; };
    set(0, -48, 0);
    set(1, -30, -10); set(2, -30, 10);   // защита: глубина 20
    set(3, -10, 0);                       // полузащита: 40
    set(4, 15, 5);                        // атака: 65
    const s = teamShape(snap, info, 0);
    expect(s.back).toBeCloseTo(20, 6);
    expect(s.mid).toBeCloseTo(40, 6);
    expect(s.front).toBeCloseTo(65, 6);
    expect(s.gapBackMid).toBeCloseTo(20, 6);
    expect(s.gapMidFront).toBeCloseTo(25, 6);
    expect(s.width).toBeCloseTo(20, 6);
    expect(s.outfield.length).toBe(4); // вратарь не входит
  });

  it("для второй команды глубина считается от её ворот на +x", () => {
    const snap = emptySnapshot(7);
    snap.players[6]!.x = 30; // защитник второй команды
    const s = teamShape(snap, info, 1);
    expect(s.back).toBeCloseTo(20, 6); // 50 - 30
  });
});
