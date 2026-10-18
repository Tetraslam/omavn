#!/usr/bin/env bash
# Point both AUR packages at a released version: pkgver, checksums, .SRCINFO.
#
#   packaging/aur/bump.sh 0.6.1
#
# The release must already be published on GitHub.
set -euo pipefail

version=${1:?usage: bump.sh <version>}
here="$(cd "$(dirname "$0")" && pwd)"
base="https://github.com/Tetraslam/omavn/releases/download/v$version"

sum() { curl -fsSL "$base/omavn-$1-unknown-linux-gnu.tar.gz.sha256" | cut -d' ' -f1; }
x86=$(sum x86_64)
arm=$(sum aarch64)
src=$(curl -fsSL "https://github.com/Tetraslam/omavn/archive/refs/tags/v$version.tar.gz" | sha256sum | cut -d' ' -f1)

bump() {
  local pkgbuild=$1
  sed -i -e "s/^pkgver=.*/pkgver=$version/" -e "s/^pkgrel=.*/pkgrel=1/" "$pkgbuild"
}

bump "$here/omavn-bin/PKGBUILD"
sed -i -e "s/^sha256sums_x86_64=.*/sha256sums_x86_64=('$x86')/" \
  -e "s/^sha256sums_aarch64=.*/sha256sums_aarch64=('$arm')/" "$here/omavn-bin/PKGBUILD"

bump "$here/omavn/PKGBUILD"
sed -i "s/^sha256sums=.*/sha256sums=('$src')/" "$here/omavn/PKGBUILD"

for pkg in omavn-bin omavn; do
  (cd "$here/$pkg" && makepkg --printsrcinfo >.SRCINFO)
done
echo "bumped to $version; publish with: packaging/aur/publish.sh"
