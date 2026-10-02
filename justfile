set shell := ["bash", "-euo", "pipefail", "-c"]

# Все проверки этапа.
check: fmt-check clippy test gen-types wasm-determinism viewer-check

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

# Сборка WASM-таргета.
wasm-check:
    cargo build -p fsim-wasm --target wasm32-unknown-unknown --release

# Тест детерминизма: хэши в WASM (node) равны нативным, для песочницы и сценария.
wasm-determinism seed="42" ticks="2000" scenario="possession_flips": wasm-check
    rm -rf target/wasm-pkg
    wasm-bindgen --target nodejs --out-dir target/wasm-pkg target/wasm32-unknown-unknown/release/fsim_wasm.wasm
    echo '{"type":"commonjs"}' > target/wasm-pkg/package.json
    node scripts/wasm-determinism.mjs target/wasm-pkg {{seed}} {{ticks}} "$(cargo run -q --release -p fsim-cli -- determinism --seed {{seed}} --ticks {{ticks}})" {{scenario}} "$(cargo run -q --release -p fsim-cli -- determinism --seed {{seed}} --ticks {{ticks}} --scenario {{scenario}})" "$(cargo run -q --release -p fsim-cli -- determinism --seed {{seed}} --ticks {{ticks}} --game)"

bench:
    cargo bench -p fsim-core

# Типы TypeScript из Rust (ts-rs).
gen-types:
    TS_RS_EXPORT_DIR="$(pwd)/viewer/src/generated" cargo test -q -p fsim-wasm export_bindings

# Сборка WASM для визуализатора.
viewer-wasm:
    cargo build -p fsim-wasm --target wasm32-unknown-unknown --release
    wasm-bindgen --target web --out-dir viewer/src/wasm target/wasm32-unknown-unknown/release/fsim_wasm.wasm

# Установка зависимостей визуализатора.
viewer-install:
    cd viewer && npm ci

# Проверка визуализатора: типы, тесты, сборка.
viewer-check: viewer-wasm gen-types
    cd viewer && npm run check

viewer-dev: viewer-wasm gen-types
    cd viewer && npm run dev
