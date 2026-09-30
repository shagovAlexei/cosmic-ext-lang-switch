# Testing

## 1. `--check`: does this machine work?

```sh
cosmic-ext-lang-switch-daemon --check
```

Prints `ok` / `MISSING` for input devices (group `input`), uinput, the COSMIC layout protocol and the configured layouts. Paste its output into any bug report.

## 2. `just verify`: fmt + clippy `-D warnings` + unit tests

The engine (`core/src/engine.rs`) holds all decision logic and is fully unit-tested. CI's stable Rust may be newer than the local one: if they disagree, believe CI.

## 3. Manual matrix (needs a live session)

| Scenario | Expected |
|---|---|
| gedit: `ghbdtn` + Insert | `привет`, panel shows RU |
| Insert again | back to `ghbdtn`, EN |
| `hello ghbdtn` + Super+Insert (release Super) | whole phrase retyped in RU |
| COSMIC Terminal, Firefox, LibreOffice: word + Insert | same; no `~` in the terminal |
| gedit: `ghbdtn world`, caret after `ghbdtn`, Insert | `привет world` (Insert is swallowed, overwrite mode stays off) |
| Super+Insert | launcher does not open |
| gedit: select `ghbdtn vbh`, Alt+Insert | `привет мир`, panel shows RU |
| gedit: select `руддщ`, Alt+Insert | `hello`, panel shows US |
| gedit: select two lines of wrong-layout text, Alt+Insert | both lines converted, line break kept |
| select text with an emoji, Alt+Insert (toggle off / on) | emoji dropped, rest converted / nothing happens |
| Caps Lock | LED follows, as before |
| normal typing, key repeat, media keys | no lag, nothing lost |
| `kill -9` the daemon | keyboard keeps working (kernel drops the grab) |
| type a word, click with touchpad elsewhere, Insert | nothing deleted |
| type a word, Super+Space, Insert | nothing happens |
| lock screen, type password, unlock, then type a word + Insert | the new word is fixed (daemon is not deaf after unlock); the password is never replayed |
| toggle off in the applet, type, Insert | nothing happens |
| popup next to COSMIC's own layout applet | same look: full layout names with codes, active one bold, dividers |
| popup: Keyboard Settings… / Region & Language… | cosmic-settings opens on that page, popup closes |
| popup: Lang Switch Settings… | the settings window opens; clicking again brings back the same window, never a second one |
| settings: record word hotkey, press F9 | saved; F9 fixes words at once, Insert no longer does |
| settings: record, press the current hotkey (Insert) | it is captured, not swallowed by the daemon |
| settings: record, press Esc / a letter / another action's combo | cancelled / hint, keeps recording / "already used" hint |
| settings: ↶ next to each hotkey | that one goes back to its default; greyed out when already default; "already used" hint on conflict |
| settings window at its default size | everything fits without scrolling, including the Status section: green dot + "running", or red dot + what is wrong (stop the service to see it) |
| settings: Language → Русский | window and popup switch to Russian at once |
| `systemctl --user stop cosmic-ext-lang-switch` | applet shows "service is not running" |

## Regression tests

| Test | What it prevents |
|---|---|
| `enter_clears_buffer_so_password_is_never_replayed` | replaying a typed password after Enter |
| `phrase_fires_after_super_released` | held Super mixing with replayed keys |
| `touch_resets_buffer` | Insert erasing text at a new touchpad-clicked position |
| `external_group_change_resets_but_own_does_not` | stale buffer after Super+Space; broken undo |
| `autorepeat_resets_buffer` | kernel autorepeat counts drifting from the app's own repeat, so Insert deletes extra text |
| `stale_modifier_after_lock_is_cleared` | Super released while locked leaving the engine deaf after unlock |
| `lock_screen_password_ended_by_click_is_not_replayed` | a lock-screen password replayed after unlock (COSMIC sends Lock but never Unlock, so there is no pause) |
| `hotkey_is_swallowed_on_press_repeat_and_release` | Insert reaching apps (overwrite mode, `~` in terminals) |
| `swallowed_hotkey_under_super_taps_f24` | a swallowed Super+Insert opening the COSMIC launcher |
| `disabled_forwards_the_hotkey` | Insert being eaten while correction is switched off |
| `latin_goes_to_next_layout`, `cyrillic_is_detected_even_if_current_layout_is_ru` | converting a selection in the wrong direction |
| `us_and_russian_share_physical_keys` (daemon) | xkb tables mapping characters to the wrong keys |
| `paused_forwards_the_hotkey_and_fixes_nothing` | the daemon swallowing the key being recorded in the settings window |
| `duplicate_hotkeys_conflict` | two actions on one combination (only one could ever fire) |
| `layouts_are_described` (daemon) | wrong names in the popup's layout list |
