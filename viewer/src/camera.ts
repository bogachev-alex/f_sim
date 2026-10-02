// Камеры: тактическая (всё поле) и следящая за мячом с приближением.

export type CameraMode = "tactical" | "follow";

/** Визуальные параметры камеры; к физике отношения не имеют. */
const MARGIN_M = 4;
const FOLLOW_ZOOM = 2.6;
/** Постоянная времени сглаживания слежения и зума, с. */
const SMOOTH_TAU_S = 0.35;

export class Camera {
  mode: CameraMode = "tactical";
  /** Центр обзора в метрах. */
  cx = 0;
  cy = 0;
  /** Пикселей на метр. */
  scale = 1;

  constructor(
    private readonly lengthM: number,
    private readonly widthM: number,
  ) {}

  /** Масштаб, при котором поле целиком помещается в холст. */
  fitScale(w: number, h: number): number {
    return Math.min(w / (this.lengthM + 2 * MARGIN_M), h / (this.widthM + 2 * MARGIN_M));
  }

  /** Сразу ставит камеру в целевое положение, без плавного перехода. */
  snap(w: number, h: number, ballX: number, ballY: number): void {
    const t = this.target(w, h, ballX, ballY);
    this.cx = t.cx;
    this.cy = t.cy;
    this.scale = t.scale;
  }

  update(dtWall: number, w: number, h: number, ballX: number, ballY: number): void {
    const t = this.target(w, h, ballX, ballY);
    const k = 1 - Math.exp(-dtWall / SMOOTH_TAU_S);
    this.cx += (t.cx - this.cx) * k;
    this.cy += (t.cy - this.cy) * k;
    // Масштаб меняем в логарифмической шкале, чтобы зум шёл с постоянной скоростью.
    this.scale *= Math.pow(t.scale / this.scale, k);
  }

  private target(w: number, h: number, ballX: number, ballY: number) {
    const fit = this.fitScale(w, h);
    if (this.mode === "tactical") return { cx: 0, cy: 0, scale: fit };
    const scale = fit * FOLLOW_ZOOM;
    // Окно не выходит за поле с запасом: смотрим на мяч, но не в пустоту.
    const halfW = w / scale / 2;
    const halfH = h / scale / 2;
    const maxX = Math.max(0, this.lengthM / 2 + MARGIN_M - halfW);
    const maxY = Math.max(0, this.widthM / 2 + MARGIN_M - halfH);
    return {
      cx: Math.min(Math.max(ballX, -maxX), maxX),
      cy: Math.min(Math.max(ballY, -maxY), maxY),
      scale,
    };
  }

  toScreen(x: number, y: number, w: number, h: number): [number, number] {
    return [w / 2 + (x - this.cx) * this.scale, h / 2 + (y - this.cy) * this.scale];
  }
}
