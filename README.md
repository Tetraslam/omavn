<p align="center">
  <img src="docs/public/logo.svg" width="96" alt="">
</p>

<h1 align="center">omavn</h1>

<p align="center">Play visual novels on Linux. Point it at whatever you downloaded and it figures out the rest.</p>

<p align="center"><a href="https://omavn.dev">omavn.dev</a> · <a href="https://omavn.dev/docs">Docs</a> · <a href="https://omavn.dev/llms.txt">llms.txt</a></p>

```bash
curl -fsSL https://omavn.dev/install.sh | sh

omavn install ~/Downloads/"Dies irae"   # three zips wrapping a split self-extracting rar
omavn run dies
```

No unzipping, no Wine prefixes to set up, no winetricks. omavn unwraps nested archives, runs installers unattended, detects the engine, and launches the game natively when it can (Ren'Py) or through Proton when it can't. Fixed-resolution games run in gamescope so they fill the screen and clicks land where you see the cursor. Each game gets art from VNDB, a library card, and an app launcher entry.

On [Omarchy](https://omarchy.org), `omavn omarchy setup` adds Visual Novels to the menu with a cover-grid library, install-finished notifications, and window rules. [Details](https://omavn.dev/docs/omarchy).

## Use

| Command | |
| --- | --- |
| `omavn install <download>` | unpack, install, and add a game |
| `omavn run <game>` | play (`--target` for other editions or config tools) |
| `omavn library` | pick from your library and play |
| `omavn list` / `info <game>` | what's installed, and where |
| `omavn set <game> <key> <value>` | display mode, default target, env vars |
| `omavn exec <game> -- winecfg` | run something in a game's prefix |
| `omavn self update` | update omavn |
| `omavn feedback` | open a prefilled bug or game report |

Every command takes `--json` for scripts and agents.

## Tested

| Game | Download shape | Runner |
| --- | --- | --- |
| Dies irae | 3 zips around an SFX split RAR | Proton + gamescope |
| Yumina the Ethereal | NSIS installer plus external `.dat` zips | Proton + gamescope |
| HimaNatsu | Ren'Py 8.5 zip | native |

## Building on omavn

The CLI is a thin layer over the `omavn-core` crate (unpacking, detection, installs, runtimes, launching, art), so launchers and GUIs can reuse it. Game manifests are plain TOML in `~/.local/share/omavn/games/*/game.toml`.

## Feedback

`omavn feedback "what happened"` opens a GitHub issue with your system details filled in. Reports of games that work are as welcome as broken ones.

## License

[Apache-2.0](LICENSE)
