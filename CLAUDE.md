# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Punto Switcher–style keyboard layout corrector for the COSMIC desktop (Pop!_OS 24.04, Wayland), written in Rust on `libcosmic`. **Load the `cosmic-applet` skill** for house conventions (libcosmic pin, justfile, config, packaging, known traps).

Phases:
- **A (manual):** a hotkey retypes the last word/phrase (or selection) in the other layout and switches the layout; panel applet shows the current layout and an on/off toggle.
- **B (auto):** detect wrong-layout words while typing and correct them on space. Separate phase, only after A is stable.

Names: crate/applet `cosmic-ext-lang-switch`, daemon `cosmic-ext-lang-switch-daemon` (`systemd --user`, user in `input` group), APP_ID `io.github.shagovAlexei.cosmic-ext-lang-switch`, D-Bus `io.github.shagovAlexei.CosmicExtLangSwitch`. Default hotkeys (configurable): word `Insert`, selection `Alt+Insert`, phrase `Super+Insert` (the machine has no Pause key; `Shift/Ctrl+Insert` are paste/copy).

Spec: `docs/superpowers/specs/2026-09-30-lang-switch-phase-a-design.md`, plans: `docs/superpowers/plans/`.

## Platform facts (verified 2026-09-30)

- Layout switching: COSMIC's Wayland protocol `zcosmic_keyboard_layout_v1` → `set_group(index)` (see `pop-os/cosmic-applets`, `cosmic-applet-input-sources`). No need to emulate the user's layout hotkey.
- Layout list: `com.system76.CosmicComp` → `xkb_config.layout` / `.variant` (comma-separated), watched via cosmic-config.
- Wayland gives no global key capture or injection: keystrokes are read from `/dev/input/event*` (evdev, needs the `input` group / udev rule) and corrections are typed via a `/dev/uinput` virtual keyboard (backspaces + replayed keycodes).
- Never capture while the session is locked or in password fields. Note that COSMIC 1.0.9 doesn't update logind `LockedHint` on lock (reported by punto-rs).

Prior art to learn from, not depend on: `netherguy4/punto-rs` (Rust, evdev/uinput, manual), `arumata/gswitch` (Go, double-Shift), `Shah-man/autoswitch` (auto mode, GPLv3).
