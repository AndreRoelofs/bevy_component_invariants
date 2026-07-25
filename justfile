# `just --list` shows everything.

_default:
    @just --list --unsorted

# Everything CI should gate on.
check: check-rust check-docs test

# Clippy across the workspace, warnings as errors.
check-rust:
    cargo clippy --workspace --all-targets -- -D warnings

# Docs must build without broken intra-doc links.
check-docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# Tests, including the doctests that verify the examples in the docs.
test:
    cargo test --workspace

# Auto-format and apply safe lint fixes.
fix:
    cargo fmt --all
    typos --write-changes

# What `cargo publish` would upload, without uploading it. The macro crate has to
# reach crates.io first — until it does, the library cannot resolve it.
package:
    cargo package -p bevy_component_invariants_macro --allow-dirty
    cargo package -p bevy_component_invariants --allow-dirty

publish:
    cargo publish -p bevy_component_invariants_macro
    cargo publish -p bevy_component_invariants
