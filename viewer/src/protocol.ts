import type { Catalog } from "./generated/Catalog";
import type { MatchInfo } from "./generated/MatchInfo";

/** Параметры нового матча. Пустые стили и схемы значат «как в сценарии». */
export interface NewMatch {
  mode: "sandbox" | "scenario" | "match";
  scenario: string;
  seed: string;
  quality: number;
  styleA: string;
  styleB: string;
  formationA: string;
  formationB: string;
}

/** Сообщения главный поток → воркер. `gen` отсекает кадры от прошлого матча. */
export type ToWorker =
  | ({ type: "new"; gen: number } & NewMatch)
  | { type: "run_to"; gen: number; tick: number };

/** Сообщения воркер → главный поток. */
export type FromWorker =
  | { type: "init"; catalog: Catalog }
  | { type: "ready"; gen: number; info: MatchInfo }
  | { type: "frames"; gen: number; buffer: ArrayBuffer; stats: string }
  | { type: "error"; gen: number; message: string };
