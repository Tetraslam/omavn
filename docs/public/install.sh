#!/bin/sh
# Install omavn: curl -fsSL https://omavn.dev/install.sh | sh
#
# Downloads the latest release from GitHub into ~/.local/bin (override with
# OMAVN_INSTALL_DIR). Later updates: `omavn self update`.
set -eu

repo="Tetraslam/omavn"
dir="${OMAVN_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) echo "omavn: unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac
[ "$(uname -s)" = Linux ] || { echo "omavn: Linux only" >&2; exit 1; }

asset="omavn-$arch-unknown-linux-gnu.tar.gz"
url="https://github.com/$repo/releases/latest/download/$asset"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "downloading $asset"
curl -fsSL "$url" -o "$tmp/$asset"
curl -fsSL "$url.sha256" -o "$tmp/$asset.sha256"
(cd "$tmp" && sha256sum -c --quiet "$asset.sha256")
tar xzf "$tmp/$asset" -C "$tmp"

mkdir -p "$dir"
install -m 755 "$tmp/omavn-$arch-unknown-linux-gnu/omavn" "$dir/omavn"
echo "installed $("$dir/omavn" --version) to $dir/omavn"

case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "add $dir to your PATH, e.g.: echo 'export PATH=\"$dir:\$PATH\"' >> ~/.bashrc" ;;
esac

if command -v omarchy-version >/dev/null 2>&1; then
  echo "on Omarchy? run: omavn omarchy setup"
fi
echo "next: omavn install ~/Downloads/<your visual novel>"
