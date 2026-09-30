# cosmic-ext-lang-switch

Punto Switcher–style layout fixer for COSMIC (Pop!_OS 24.04, Wayland): press a hotkey and the last word or phrase typed in the wrong layout is retyped in the other one.

Default hotkeys (configurable in the settings window: popup → Lang Switch Settings…, or `cosmic-ext-lang-switch --settings`): **Insert** fixes the last word, **Super+Insert** the phrase (press again to undo), **Alt+Insert** the selected text.

## Install

```sh
sudo usermod -aG input $USER      # then log out and in
just build-release && sudo just install
systemctl --user daemon-reload && systemctl --user enable --now cosmic-ext-lang-switch
cosmic-ext-lang-switch-daemon --check
```
Then add "Lang Switch" in Settings → Desktop → Panel → Applets (restart the panel with `killall cosmic-panel` if it is not listed).

The daemon reads `/dev/input` and types through `/dev/uinput`: it needs the `input` group, so it cannot be a Flatpak.

Security note: the udev rule gives the `input` group write access to `/dev/uinput`, so any process of that user can inject keystrokes.

The daemon grabs keyboards (`EVIOCGRAB`) and forwards every key except the hotkey through its virtual keyboard, so the hotkey never reaches applications. If the daemon dies, the kernel releases the grab and the keyboard works directly again. With keyd configured, keyd holds the physical keyboard and this daemon grabs keyd's virtual keyboard instead.

Selection fixing works where the selection is editable (text fields, editors). In a terminal the selection is not editable: use Insert / Super+Insert there. Characters no layout can type (emoji) are left out of a converted selection, or the whole selection is left alone if "Skip fixing a selection with characters no layout can type" is on.

## Auto-correction (off by default)

Turn on "Auto-correct while typing" in the popup or in the settings window. When you finish a word with Space and it was clearly typed in the wrong layout (`ghbdtn`), it is retyped in the other one (`привет`) and the layout is switched. Press Insert right after to undo; the word is then remembered and never auto-corrected again.

It never touches words shorter than 3 letters, words with digits or mixed case (`myVar`), words ending in punctuation, dictionary words (hunspell `en_US` / `ru_RU` from `/usr/share/hunspell`), and anything typed in excluded apps (terminals and code editors by default; editable in settings). Excluding apps needs the applet in the panel: only panel applets are told which window is focused.

**Passwords:** a password field can't be detected. A password of lowercase letters only that reads like Russian on the Russian layout (or vice versa) may get auto-corrected. Keep auto-correction off if that matters to you, or add the app to the exclusions.

The language models (`core/data/trigrams-*.bin`) are built by `cargo run -p gen-model` from the hunspell dictionaries; `cargo run --release -p gen-model -- --eval` reports the detection and false-positive rates for a grid of thresholds.
