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

# sudo just install; the applet starts the daemon (re-add it to the panel or log in again)
install:
    install -Dm0755 target/release/{{name}} {{prefix}}/bin/{{name}}
    install -Dm0755 target/release/{{name}}-daemon {{prefix}}/bin/{{name}}-daemon
    install -Dm0644 res/{{APPID}}.desktop {{prefix}}/share/applications/{{APPID}}.desktop
    install -Dm0644 res/{{APPID}}.metainfo.xml {{prefix}}/share/metainfo/{{APPID}}.metainfo.xml
    install -Dm0644 res/icons/hicolor/scalable/apps/{{APPID}}.svg {{prefix}}/share/icons/hicolor/scalable/apps/{{APPID}}.svg
    rm -f {{prefix}}/lib/systemd/user/{{name}}.service  # left by versions before 0.1.0
    install -Dm0644 res/60-{{name}}.rules {{prefix}}/lib/udev/rules.d/60-{{name}}.rules
    udevadm control --reload && udevadm trigger --sysname-match=uinput

uninstall:
    rm -f {{prefix}}/bin/{{name}} {{prefix}}/bin/{{name}}-daemon {{prefix}}/share/applications/{{APPID}}.desktop {{prefix}}/share/metainfo/{{APPID}}.metainfo.xml {{prefix}}/share/icons/hicolor/scalable/apps/{{APPID}}.svg {{prefix}}/lib/udev/rules.d/60-{{name}}.rules

run-daemon:
    env RUST_LOG=info cargo run -p {{name}}-daemon
