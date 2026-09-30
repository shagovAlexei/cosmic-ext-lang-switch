# A2 — план

Spec: `docs/superpowers/specs/2026-09-30-selection-design.md`. Ветка `selection`, результат через PR.

- [x] **Task 1: core.** `hotkey`: `Scope::Selection`, `Hotkeys.selection`, `DEFAULT_SELECTION = "Alt+Insert"`. `engine`: `Action::ConvertSelection`. `selection.rs`: `Table`, `Unknown`, `convert` (TDD). `config`: `hotkey_selection`, `abort_on_unknown`.
- [x] **Task 2: daemon.** `tables.rs` (xkbcommon → `Vec<Table>`), чтение primary (`wl-clipboard-rs`, spawn_blocking + timeout), обработка `ConvertSelection` в `exec`. `--check`: строка `selection` (data-control доступен).
- [x] **Task 3: applet.** Третье поле хоткея, переключатель abort; строки en/ru.
- [ ] **Task 4: docs + live.** TESTING.md, README; ручная проверка пользователем; PR.
