#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "[feature-matrix] cargo check --no-default-features"
cargo check --no-default-features

echo "[feature-matrix] cargo check --no-default-features --features shell,rt-compio"
cargo check --no-default-features --features shell,rt-compio

echo "[feature-matrix] cargo check --no-default-features --features shell,rt-tokio"
cargo check --no-default-features --features shell,rt-tokio

echo "[feature-matrix] downstream fixture compile without local shell feature"
cargo check --manifest-path tests/fixtures/downstream-subscription/Cargo.toml

echo "[feature-matrix] derive regression compile test"
cargo test --test compile_fail derive_model_subscription_parts_compile_in_downstream_crate -- --nocapture
