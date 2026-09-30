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
| COSMIC Terminal, Firefox, LibreOffice: word + Insert | same (LibreOffice: Insert also toggles overwrite mode) |
| type a word, click with touchpad elsewhere, Insert | nothing deleted |
| type a word, Super+Space, Insert | nothing happens |
| lock screen, type password, unlock, then type a word + Insert | the new word is fixed (daemon is not deaf after unlock); the password is never replayed |
| toggle off in the applet, type, Insert | nothing happens |
| set word hotkey to F9 in the applet | F9 works at once, Insert no longer does |
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
