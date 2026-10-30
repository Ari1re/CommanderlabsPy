@echo off
cargo clean
cargo build
cargo run --release
pause