#!/bin/sh
# Builds plugin.wasm next to plugin.toml, so this folder is the plug-in.
# Needs: rustup target add wasm32-unknown-unknown
set -e
cd "$(dirname "$0")"
cargo build --release --locked --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/popamp_plugin_youtube.wasm plugin.wasm
