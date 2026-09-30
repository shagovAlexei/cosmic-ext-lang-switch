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

prefix := '/usr'

build-release:
    cargo build --release -p {{name}} -p {{name}}-daemon

# sudo just install; then: systemctl --user daemon-reload && systemctl --user enable --now {{name}}
install:
    install -Dm0755 target/release/{{name}} {{prefix}}/bin/{{name}}
    install -Dm0755 target/release/{{name}}-daemon {{prefix}}/bin/{{name}}-daemon
    install -Dm0644 res/{{APPID}}.desktop {{prefix}}/share/applications/{{APPID}}.desktop
    install -Dm0644 res/{{name}}.service {{prefix}}/lib/systemd/user/{{name}}.service
    install -Dm0644 res/60-{{name}}.rules {{prefix}}/lib/udev/rules.d/60-{{name}}.rules
    udevadm control --reload && udevadm trigger --sysname-match=uinput

uninstall:
    rm -f {{prefix}}/bin/{{name}} {{prefix}}/bin/{{name}}-daemon {{prefix}}/share/applications/{{APPID}}.desktop {{prefix}}/lib/systemd/user/{{name}}.service {{prefix}}/lib/udev/rules.d/60-{{name}}.rules

run-daemon:
    env RUST_LOG=info cargo run -p {{name}}-daemon
