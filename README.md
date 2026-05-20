# monitor-switcher

Auto-switch secondary monitor inputs based on a master DDC/CI display's active source.

Polls VCP `0x60` on a master monitor. When the value matches a configured profile trigger, applies the profile's target writes to secondary monitors. Includes a KDE Plasma / Wayland tray icon with force-switch radios, pause toggle, event log, and quit.

[![CI](https://github.com/saikhurana98/monitor-switcher/actions/workflows/ci.yml/badge.svg)](https://github.com/saikhurana98/monitor-switcher/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Why

I have three monitors. The Dell P3425WE is my "master" — it has two inputs (HDMI from my PC, USB-C from my docked laptop) and switches between them via its KVM. The other two monitors are dumb: they don't follow the KVM. This daemon watches the Dell over DDC/CI, and when it flips, writes the matching inputs to the secondaries.

The polling architecture is intentional: DDC/CI has no event push, the Dell does not expose KVM-state over USB-HID, and reading VCP `0x60` is cheap.

## Requirements

- Linux with `i2c-dev` kernel module (default on most desktops)
- `ddcutil` on `PATH`
- DDC/CI-capable monitors discoverable via `ddcutil detect`
- Read/write access to `/dev/i2c-*`: systemd-logind `uaccess` covers an interactive session; system services need the `i2c` group
- KDE Plasma 5/6, or any StatusNotifierItem host (waybar, swaync, etc.) for the tray icon

## Install

### From release tarball (recommended)

```sh
VERSION=0.1.0
ARCH=x86_64-unknown-linux-gnu
curl -L -o ms.tar.gz \
  "https://github.com/saikhurana98/monitor-switcher/releases/download/v${VERSION}/monitor-switcher-${ARCH}.tar.gz"
tar -xzf ms.tar.gz
install -Dm755 monitor-switcher/monitor-switcher ~/.local/bin/monitor-switcher
install -Dm644 monitor-switcher/systemd/monitor-switcher.service ~/.config/systemd/user/monitor-switcher.service
mkdir -p ~/.config/monitor-switcher
cp monitor-switcher/config.example.yaml ~/.config/monitor-switcher/config.yaml
```

### From source

```sh
cargo install --git https://github.com/saikhurana98/monitor-switcher
```

### Arch Linux (AUR)

```sh
# from source
yay -S monitor-switcher
# prebuilt
yay -S monitor-switcher-bin
```

### From crates.io

```sh
cargo install monitor-switcher
```

## Configure

```sh
mkdir -p ~/.config/monitor-switcher
cp config.example.yaml ~/.config/monitor-switcher/config.yaml
```

Edit `~/.config/monitor-switcher/config.yaml` to match your buses. Find them with `ddcutil detect`.

### Example

```yaml
poll_interval_seconds: 1

master:
  bus: 5
  name: "Dell P3425WE"

profiles:
  pc:
    label: "PC (HDMI)"
    trigger: 0x11  # master sees HDMI-1
    targets:
      - { bus: 4, value: 0x11, name: "Acer → HDMI-1" }
      - { bus: 3, value: 0x11, name: "Lenovo → HDMI-1" }

  laptop:
    label: "Laptop (USB-C)"
    trigger: 0x1B  # Dell proprietary USB-C upstream
    targets:
      - { bus: 4, value: 0x12, name: "Acer → HDMI-2" }
      - { bus: 3, value: 0x0F, name: "Lenovo → DisplayPort" }
```

MCCS input codes (VCP `0x60`):
- `0x0F` DisplayPort-1
- `0x10` DisplayPort-2
- `0x11` HDMI-1
- `0x12` HDMI-2
- `0x1B` Dell-proprietary USB-C upstream
- vendor-specific values exist; verify with `ddcutil --bus N getvcp 60`

## Run

```sh
monitor-switcher --config ~/.config/monitor-switcher/config.yaml
```

One-shot mode (no tray, single poll for testing):

```sh
monitor-switcher --once --config ~/.config/monitor-switcher/config.yaml
```

## Autostart (systemd user service)

```sh
systemctl --user daemon-reload
systemctl --user enable --now monitor-switcher.service
journalctl --user -u monitor-switcher -f
```

## Troubleshooting

- **`ddcutil: command not found`** — install ddcutil (`pacman -S ddcutil`, `apt install ddcutil`).
- **`Permission denied: /dev/i2c-N`** — confirm `loginctl show-session $XDG_SESSION_ID -p Active` is `yes`; uaccess only applies to an active session. For headless/service use, add yourself to the `i2c` group and reboot.
- **No tray icon** — confirm your panel hosts StatusNotifierItem. KDE Plasma does by default. GNOME requires the AppIndicator extension.
- **Master never flips** — verify with `watch -n1 'ddcutil --bus N getvcp 60 --brief'` while you switch the KVM physically.
- **Secondaries flicker** — increase `poll_interval_seconds`. The per-target read-back already deduplicates redundant writes; flicker means your monitor's DDC firmware is slow.

## Develop

```sh
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

## License

MIT. See [LICENSE](LICENSE).
