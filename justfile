name := 'cosmic-ext-lang-switch'
export APPID := 'io.github.shagovAlexei.cosmic-ext-lang-switch'

[private]
default:
    @just --list

# fmt + clippy -D warnings + tests — what CI runs
verify:
    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings
    cargo test --all

check *args:
    cargo fmt --all
    cargo clippy --all-targets {{args}} -- -W clippy::pedantic
