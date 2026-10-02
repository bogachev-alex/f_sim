// Часы воспроизведения: переводят реальное время в тики матча с выбранной скоростью.

export const SPEEDS = [0.25, 0.5, 1, 2, 4, 8, 16] as const;

export class Playback {
  /** Текущее дробное положение в тиках. */
  tick = 0;
  speed = 1;
  paused = false;

  constructor(private readonly tickHz: number) {}

  /** Двигает часы на `wallSeconds` реального времени, но не дальше `limit` (последний готовый кадр). */
  advance(wallSeconds: number, limit: number): void {
    if (this.paused) return;
    this.tick = Math.min(this.tick + wallSeconds * this.speed * this.tickHz, limit);
  }

  /** Шаг на один тик: ставит на паузу и сдвигает ровно на тик вперёд. */
  stepOnce(limit: number): void {
    this.paused = true;
    this.tick = Math.min(Math.floor(this.tick) + 1, limit);
  }

  reset(): void {
    this.tick = 0;
  }

  seconds(): number {
    return this.tick / this.tickHz;
  }

  /** Сколько тиков нужно иметь впереди, чтобы интерполяция не упиралась в конец данных. */
  leadTicks(): number {
    return Math.max(10, Math.ceil(this.speed * this.tickHz * 0.25));
  }
}
