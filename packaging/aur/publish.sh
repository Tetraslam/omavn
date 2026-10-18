#!/usr/bin/env bash
# Push the AUR packages to aur.archlinux.org. Needs an AUR account with
# your SSH key; the first push creates each package.
#
#   packaging/aur/publish.sh [omavn-bin] [omavn]
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
pkgs=("$@")
((${#pkgs[@]})) || pkgs=(omavn-bin omavn)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

for pkg in "${pkgs[@]}"; do
  git clone -q "ssh://aur@aur.archlinux.org/$pkg.git" "$work/$pkg"
  cp "$here/$pkg/PKGBUILD" "$here/$pkg/.SRCINFO" "$work/$pkg/"
  version=$(sed -n 's/^pkgver=//p' "$here/$pkg/PKGBUILD")
  (
    cd "$work/$pkg"
    git add PKGBUILD .SRCINFO
    if git diff --cached --quiet; then
      echo "$pkg: already at $version"
    else
      git commit -q -m "Update to $version"
      git push -q origin HEAD:master
      echo "$pkg: published $version"
    fi
  )
done
