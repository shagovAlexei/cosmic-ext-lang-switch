# cosmic-ext-lang-switch

Punto Switcher–style layout fixer for COSMIC (Pop!_OS 24.04, Wayland): press a hotkey and the last word or phrase typed in the wrong layout is retyped in the other one.

Default hotkeys (configurable in the applet): **Insert** fixes the last word, **Super+Insert** the phrase; press again to undo.

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
