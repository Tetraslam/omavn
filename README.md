# omavn

Play visual novels on Linux. Point it at whatever you downloaded and it figures out the rest.

```bash
omavn install ~/Downloads/"Dies irae"   # three zips wrapping a split self-extracting rar
omavn run dies-irae
```

No unzipping, no Wine prefixes to set up, no winetricks. omavn unwraps nested archives, runs installers unattended, detects the engine, and launches the game natively when it can (Ren'Py) or through Proton when it can't.

## Install

```bash
cargo install --git https://github.com/Tetraslam/omavn omavn
```

Needs `python3` (for umu-launcher). Proton, umu-launcher, and Ren'Py runtimes are fetched on demand, or reused from Steam if you have them. Run `omavn doctor` to check your setup.

## Use

| Command | |
| --- | --- |
| `omavn install <download>` | unpack, install, and add a game |
| `omavn run <game>` | play (`--target` for other editions or config tools) |
| `omavn list` / `info <game>` | what's installed, and where |
| `omavn exec <game> -- winecfg` | run something in a game's prefix |
| `omavn feedback` | open a prefilled bug or game report |

Every command takes `--json` for scripts and agents.

## Tested

| Game | Download shape | Runner |
| --- | --- | --- |
| Dies irae | 3 zips around an SFX split RAR | Proton |
| Yumina the Ethereal | NSIS installer plus external `.dat` zips | Proton |
| HimaNatsu | Ren'Py 8.5 zip | native |

## Docs

Full docs live in [`docs/`](docs/content/docs) (a [Fumadocs](https://fumadocs.dev) site): how it works, the per-game catalog, troubleshooting, and using `omavn-core` as a library.

## Feedback

`omavn feedback "what happened"` opens a GitHub issue with your system details filled in. Reports of games that work are as welcome as broken ones.
