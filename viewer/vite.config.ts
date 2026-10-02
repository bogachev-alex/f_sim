import { defineConfig } from "vitest/config";

export default defineConfig({
  // На GitHub Pages сайт лежит в подкаталоге `/<репозиторий>/`.
  base: process.env.BASE_PATH ?? "/",
  build: { target: "es2022" },
  worker: { format: "es" },
  test: { environment: "node", include: ["src/**/*.test.ts"] },
});
