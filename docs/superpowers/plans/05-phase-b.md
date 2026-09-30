# Фаза B — план

Spec: `docs/superpowers/specs/2026-10-01-phase-b-auto-design.md`. Ветка `phase-b`. Коммиты только после подтверждения пользователя (CLAUDE.md).

- [x] **Task 1: spike — активное окно.** (результат в спеке: активное окно отслеживает аплет) Выбросной клиент: получить `app_id` активного окна через `zcosmic_toplevel_info_v1`/`ext_foreign_toplevel_list_v1` от обычного пользовательского процесса. Результат — строка в спеке.
- [x] **Task 2: модель.** `tools/gen-model` (bin в workspace): триграммы en/ru → `core/data/trigrams.bin`; формат и загрузчик в `core/src/auto.rs` (TDD на загрузчике); отчёт о точности.
- [x] **Task 3: решение.** (MARGIN 2.0, FLOOR −6.0: 97.8% caught, 0.014% false+ in-sample; + встроенное вето TECH_WORDS: http→реез не отделить порогом) `auto::decide(word_strokes, tables, current, model, veto, exceptions) -> Option<u32>` с правилами из спеки (TDD на контрольных словах); подбор `MARGIN`/`FLOOR` по отчёту, тест на порог точности.
- [x] **Task 4: движок.** (+ слова, оканчивающиеся на `,.;:!?`, пропускаются: эти клавиши — русские буквы) Режим auto на пробеле, отмена → `Action::AddException`, флаг «приложение исключено» (TDD).
- [x] **Task 5: демон.** Загрузка словарей для вето; D-Bus `SetActiveApp(s)` (активное окно присылает аплет); запись исключений в конфиг; конфиг `auto_*`.
- [ ] **Task 6: аплет.** `X-HostWaylandDisplay=true`, отслеживание активного окна через привилегированное подключение → `SetActiveApp`; переключатель в попапе; вкладки настроек и списки; строки en/ru.
- [ ] **Task 7: docs + live.** README (предупреждение о паролях), TESTING.md; ручная проверка; по подтверждению — коммит, PR, merge.
