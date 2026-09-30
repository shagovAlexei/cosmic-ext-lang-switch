# Перехват хоткея — план

Spec: `docs/superpowers/specs/2026-09-30-grab-hotkey-design.md`. Ветка `grab-hotkey`, результат через PR.

- [x] **Task 1: core.** `Outcome` + `Engine::feed` (TDD: 5 тестов из спеки). Файл `core/src/engine.rs`; `keys::F24 = 194`.
- [x] **Task 2: daemon.** `input.rs`: `is_keyboard`, захват клавиатур при `grab = true`, канал `(code, value, grabbed)`, расширенный набор клавиш виртуальной клавиатуры. `main.rs`: `engine.feed`, пересылка до исправления, `tap_instead`. `--check` не захватывает.
- [ ] **Task 3: docs + live.** TESTING.md (новые ручные сценарии и регрессионные тесты), README (keyd-цепочка, аварийный выход). Установка и ручная проверка пользователем, затем PR.
