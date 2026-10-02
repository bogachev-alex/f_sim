// Панель управления: воспроизведение, скорость, камера, слои, параметры нового матча.
import type { Catalog } from "./generated/Catalog";
import type { CameraMode } from "./camera";
import { SPEEDS } from "./playback";
import type { NewMatch } from "./protocol";

type LayerKey = "trails" | "targets" | "numbers" | "anchors" | "lines" | "hull" | "duties";

const LAYERS: { key: LayerKey; label: string; title: string; on: boolean }[] = [
  { key: "numbers", label: "Номера", title: "Номера игроков", on: true },
  { key: "anchors", label: "Якоря", title: "Якорные точки и линии к ним", on: false },
  { key: "lines", label: "Линии", title: "Линия обороны (сплошная) и офсайда (штриховая)", on: false },
  { key: "hull", label: "Контур", title: "Контур формы: выпуклая оболочка полевых игроков", on: false },
  { key: "duties", label: "Оборона", title: "Роли в обороне: П прессинг, Т тень, С страховка, О опека, К контрпрессинг, М владелец", on: false },
  { key: "trails", label: "Следы", title: "Следы за последние секунды", on: false },
  { key: "targets", label: "Цели", title: "Цели движения игроков", on: false },
];

export interface UiState extends NewMatch {
  paused: boolean;
  speed: number;
  camera: CameraMode;
  layers: Record<LayerKey, boolean>;
}

export interface UiHandlers {
  onPlayPause(): void;
  onStep(): void;
  onSpeed(speed: number): void;
  onCamera(mode: CameraMode): void;
  onNewMatch(): void;
  onRestart(): void;
}

const DEFAULT_SCENARIO = "ball_sweep";

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> & { class?: string } = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  const { class: cls, ...rest } = props;
  if (cls) e.className = cls;
  Object.assign(e, rest);
  e.append(...children);
  return e;
}

function labeled(text: string, control: HTMLElement): HTMLLabelElement {
  return el("label", {}, text, control);
}

function fill(sel: HTMLSelectElement, options: [string, string][]): void {
  sel.replaceChildren(...options.map(([value, text]) => el("option", { value, textContent: text })));
}

export class Ui {
  readonly state: UiState = {
    paused: false,
    speed: 1,
    camera: "tactical",
    layers: Object.fromEntries(LAYERS.map((l) => [l.key, l.on])) as Record<LayerKey, boolean>,
    mode: "match",
    scenario: DEFAULT_SCENARIO,
    seed: "42",
    quality: 0.62,
    styleA: "",
    styleB: "",
    formationA: "",
    formationB: "",
  };

  private catalog: Catalog | null = null;
  private readonly playBtn: HTMLButtonElement;
  private readonly speedSel: HTMLSelectElement;
  private readonly camSel: HTMLSelectElement;
  private readonly modeSel: HTMLSelectElement;
  private readonly scenarioSel: HTMLSelectElement;
  private readonly seedInput: HTMLInputElement;
  private readonly qualityInput: HTMLInputElement;
  private readonly qualityValue: HTMLSpanElement;
  private readonly styleA: HTMLSelectElement;
  private readonly styleB: HTMLSelectElement;
  private readonly formA: HTMLSelectElement;
  private readonly formB: HTMLSelectElement;
  private readonly checks = new Map<LayerKey, HTMLInputElement>();

  constructor(root: HTMLElement, h: UiHandlers) {
    this.playBtn = el("button", { type: "button", title: "Пауза или продолжение (пробел)" });
    const stepBtn = el("button", { type: "button", textContent: "Шаг", title: "Один тик вперёд (→)" });
    this.speedSel = el("select", { title: "Скорость воспроизведения" });
    fill(this.speedSel, SPEEDS.map((s) => [String(s), `${s}×`]));
    this.speedSel.value = "1";
    this.camSel = el("select", { title: "Камера" });
    fill(this.camSel, [["tactical", "Всё поле"], ["follow", "За мячом"]]);

    this.modeSel = el("select", { title: "Режим: сценарий формы или физическая песочница" });
    fill(this.modeSel, [["match", "Матч"], ["scenario", "Сценарий"], ["sandbox", "Песочница"]]);
    this.scenarioSel = el("select", { title: "Сценарий" });
    this.styleA = el("select", { title: "Стиль команды A" });
    this.styleB = el("select", { title: "Стиль команды B" });
    this.formA = el("select", { title: "Схема команды A" });
    this.formB = el("select", { title: "Схема команды B" });

    const layerLabels: HTMLLabelElement[] = [];
    for (const l of LAYERS) {
      const input = el("input", { type: "checkbox", checked: l.on, title: l.title });
      this.checks.set(l.key, input);
      layerLabels.push(labeled("", input));
      layerLabels[layerLabels.length - 1]!.append(l.label);
      input.addEventListener("change", () => {
        this.state.layers[l.key] = input.checked;
      });
    }

    this.seedInput = el("input", { type: "number", min: "0", step: "1", value: this.state.seed, title: "Seed матча" });
    const randomBtn = el("button", { type: "button", textContent: "Случайный", title: "Случайный seed" });
    this.qualityInput = el("input", { type: "range", min: "0.30", max: "0.90", step: "0.01", value: String(this.state.quality) });
    this.qualityValue = el("span", { textContent: this.state.quality.toFixed(2) });
    const newBtn = el("button", { type: "button", class: "primary", textContent: "Новый матч" });
    const restartBtn = el("button", { type: "button", textContent: "Перезапуск", title: "Тот же seed с начала" });

    root.append(
      el("div", { class: "group" }, this.playBtn, stepBtn, labeled("Скорость", this.speedSel)),
      el("div", { class: "group" }, labeled("Режим", this.modeSel), labeled("Сценарий", this.scenarioSel)),
      el("div", { class: "group" }, labeled("Стиль A", this.styleA), labeled("Схема A", this.formA),
        labeled("Стиль B", this.styleB), labeled("Схема B", this.formB)),
      el("div", { class: "group" }, labeled("Камера", this.camSel), ...layerLabels),
      el("div", { class: "group" }, labeled("Seed", this.seedInput), randomBtn,
        labeled("Качество", this.qualityInput), this.qualityValue, newBtn, restartBtn),
    );

    this.playBtn.addEventListener("click", () => h.onPlayPause());
    stepBtn.addEventListener("click", () => h.onStep());
    this.speedSel.addEventListener("change", () => {
      this.state.speed = Number(this.speedSel.value);
      h.onSpeed(this.state.speed);
    });
    this.camSel.addEventListener("change", () => {
      this.state.camera = this.camSel.value as CameraMode;
      h.onCamera(this.state.camera);
    });
    this.modeSel.addEventListener("change", () => {
      this.readMatchParams();
      this.syncMode();
      h.onNewMatch();
    });
    this.scenarioSel.addEventListener("change", () => {
      this.readMatchParams();
      this.applyScenarioDefaults();
      h.onNewMatch();
    });
    randomBtn.addEventListener("click", () => {
      this.seedInput.value = String(Math.floor(Math.random() * 2 ** 31));
    });
    this.qualityInput.addEventListener("input", () => {
      this.state.quality = Number(this.qualityInput.value);
      this.qualityValue.textContent = this.state.quality.toFixed(2);
    });
    newBtn.addEventListener("click", () => {
      this.readMatchParams();
      h.onNewMatch();
    });
    restartBtn.addEventListener("click", () => {
      this.readMatchParams();
      h.onRestart();
    });

    window.addEventListener("keydown", (e) => {
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA") return;
      if (e.code === "Space") {
        e.preventDefault();
        h.onPlayPause();
      } else if (e.code === "ArrowRight") {
        h.onStep();
      }
    });
    this.refresh();
  }

  /** Каталог схем, стилей и сценариев приходит из движка. */
  setCatalog(c: Catalog): void {
    this.catalog = c;
    fill(this.scenarioSel, c.scenarios.map((s) => [s.id, s.id]));
    this.scenarioSel.value = c.scenarios.some((s) => s.id === DEFAULT_SCENARIO) ? DEFAULT_SCENARIO : (c.scenarios[0]?.id ?? "");
    this.syncMode();
    this.readMatchParams();
  }

  /** Списки стилей и схем с вариантом «как в сценарии» и подписью по умолчанию. */
  private applyScenarioDefaults(): void {
    const c = this.catalog;
    if (!c) return;
    const sc = c.scenarios.find((s) => s.id === this.scenarioSel.value);
    this.scenarioSel.title = sc?.description ?? "Сценарий";
    const scenarioMode = this.modeSel.value === "scenario";
    const label = (def: string | undefined) =>
      def ? `${scenarioMode ? "как в сценарии" : "по умолчанию"} (${def})` : "по умолчанию";
    const styleOpts = (def: string | undefined): [string, string][] => [
      ["", label(def)],
      ...c.styles.map((s): [string, string] => [s.id, s.id]),
    ];
    const formOpts = (def: string | undefined): [string, string][] => [
      ["", label(def)],
      ...c.formations.map((f): [string, string] => [f, f]),
    ];
    const mode = this.modeSel.value;
    const scenario = mode === "scenario";
    fill(this.styleA, styleOpts(scenario ? sc?.styles[0] : mode === "match" ? "positional" : undefined));
    fill(this.styleB, styleOpts(scenario ? sc?.styles[1] : mode === "match" ? "gegenpress" : undefined));
    fill(this.formA, formOpts(scenario ? sc?.formations[0] : "4-3-3"));
    fill(this.formB, formOpts(scenario ? sc?.formations[1] : mode === "match" ? "4-3-3" : "4-4-2"));
  }

  private syncMode(): void {
    const mode = this.modeSel.value;
    this.scenarioSel.disabled = mode !== "scenario";
    this.styleA.disabled = mode === "sandbox";
    this.styleB.disabled = mode === "sandbox";
    this.applyScenarioDefaults();
  }

  readMatchParams(): void {
    const s = this.state;
    s.mode = this.modeSel.value as NewMatch["mode"];
    s.scenario = this.scenarioSel.value;
    s.seed = String(Math.max(0, Math.floor(Number(this.seedInput.value) || 0)));
    s.styleA = this.styleA.value;
    s.styleB = this.styleB.value;
    s.formationA = this.formA.value;
    s.formationB = this.formB.value;
  }

  /** Синхронизирует подписи с состоянием. */
  refresh(): void {
    this.playBtn.textContent = this.state.paused ? "Играть" : "Пауза";
    this.speedSel.value = String(this.state.speed);
    this.camSel.value = this.state.camera;
  }
}
