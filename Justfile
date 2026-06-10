# mesh-console developer commands

# Fast logic tests: console-types + console-core without the embedded runtime.
test:
    cargo test -p console-types
    cargo test -p console-core --no-default-features

# Real embedded-node smoke test (opens localhost sockets; no model downloads).
test-node:
    cargo test -p console-core --test embedded_node_smoke -- --ignored

# Heavy end-to-end: downloads the native runtime + a small GGUF and runs chat.
test-heavy:
    MESH_CONSOLE_HEAVY_TESTS=1 cargo test -p console-core -- --ignored heavy

lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo fmt --all --check

fmt:
    cargo fmt --all

check:
    cargo check --workspace

run:
    cargo run -p mesh-console

bundle:
    dx bundle --package mesh-console --release
