#!/bin/sh
# slopcop installer for Linux and macOS.
#
# Downloads one released slopcop binary from GitHub over HTTPS, verifies it
# against the checksum published next to it, and installs it to
# ~/.local/bin (no root). Nothing else is downloaded or executed.
#
# Environment:
#   SLOPCOP_SOURCE    set to 1 to build the current source from GitHub
#                     instead of downloading a release (needs cargo)
#   SLOPCOP_VERSION   release tag to install, e.g. v0.1.1, or with
#                     SLOPCOP_SOURCE the branch, tag or commit to build
#                     (default: latest release, main from source)
#   SLOPCOP_PREFIX    install directory (default: ~/.local/bin)
#   SLOPCOP_BASE_URL  alternative release directory (https:// or file://),
#                     for mirrors and tests; needs SLOPCOP_VERSION
set -eu

REPO="JGalego/slopcop"
VERSION="${SLOPCOP_VERSION:-latest}"
PREFIX="${SLOPCOP_PREFIX:-$HOME/.local/bin}"

say() { printf 'slopcop-install: %s\n' "$*"; }
die() { printf 'slopcop-install: error: %s\n' "$*" >&2; exit 1; }

# install_binary puts one built or downloaded slopcop in place, replacing any
# copy already there atomically so the script is safe to re-run.
install_binary() {
  mkdir -p "$PREFIX"
  cp "$1" "$PREFIX/.slopcop.new"
  chmod 755 "$PREFIX/.slopcop.new"
  mv -f "$PREFIX/.slopcop.new" "$PREFIX/slopcop"
  say "installed $PREFIX/slopcop ($("$PREFIX/slopcop" --version))"
  case ":$PATH:" in
    *":$PREFIX:"*) ;;
    *) say "add $PREFIX to your PATH, e.g.: echo 'export PATH=\"$PREFIX:\$PATH\"' >> ~/.profile" ;;
  esac
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

if [ "${SLOPCOP_SOURCE:-0}" = 1 ]; then
  command -v cargo >/dev/null 2>&1 || die "building from source needs cargo: https://rustup.rs"
  if [ "$VERSION" = latest ]; then
    say "building https://github.com/$REPO (main) with $(cargo --version)"
    set --
  else
    say "building https://github.com/$REPO ($VERSION) with $(cargo --version)"
    set -- --rev "$VERSION"
  fi
  cargo install --locked --root "$tmp" --git "https://github.com/$REPO" "$@" slopcop ||
    die "build failed; check that $VERSION exists in https://github.com/$REPO"
  install_binary "$tmp/bin/slopcop"
  exit 0
fi

case "$(uname -s)" in
  Linux)
    # Release binaries link against glibc.
    if ldd --version 2>&1 | grep -qi musl; then
      die "no prebuilt binary for musl libc; install with: cargo install slopcop"
    fi
    os=unknown-linux-gnu
    ;;
  Darwin) os=apple-darwin ;;
  *) die "unsupported OS $(uname -s); on Windows use install.ps1" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) die "unsupported CPU architecture $(uname -m) (need x86_64 or arm64)" ;;
esac
# Under Rosetta, uname reports x86_64 on Apple silicon; prefer the native build.
if [ "$os" = apple-darwin ] && [ "$arch" = x86_64 ] &&
  [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = 1 ]; then
  arch=aarch64
fi

if command -v curl >/dev/null 2>&1; then
  fetch() { curl --proto '=https,file' --tlsv1.2 -fsSL "$1" -o "$2"; }
  # The latest-release page redirects to the page of its tag.
  latest_tag() {
    curl --proto '=https' --tlsv1.2 -fsSLI -o /dev/null -w '%{url_effective}' \
      "https://github.com/$REPO/releases/latest" | sed 's|.*/tag/||'
  }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget --https-only -q "$1" -O "$2"; }
  latest_tag() {
    wget --https-only -qO- "https://api.github.com/repos/$REPO/releases/latest" |
      sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p'
  }
else
  die "curl or wget is required"
fi
if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  die "sha256sum or shasum is required to verify the download"
fi
command -v tar >/dev/null 2>&1 || die "tar is required"

if [ "$VERSION" = latest ]; then
  [ -z "${SLOPCOP_BASE_URL:-}" ] || die "SLOPCOP_BASE_URL needs SLOPCOP_VERSION"
  VERSION="$(latest_tag)" || die "could not look up the latest release"
  case "$VERSION" in
    v*) ;;
    *) die "could not look up the latest release" ;;
  esac
fi
base="${SLOPCOP_BASE_URL:-https://github.com/$REPO/releases/download/$VERSION}"
case "$base" in
  https://* | file://*) ;;
  *) die "refusing non-HTTPS download location: $base" ;;
esac

asset="slopcop-$VERSION-$arch-$os.tar.gz"
say "downloading $asset from $base"
fetch "$base/$asset" "$tmp/$asset" || die "download failed: $base/$asset"
fetch "$base/${asset%.tar.gz}.sha256" "$tmp/sha256" || die "download failed: $base/${asset%.tar.gz}.sha256"

want="$(cut -d' ' -f1 "$tmp/sha256")"
got="$(sha256 "$tmp/$asset")"
[ "$want" = "$got" ] || die "checksum mismatch for $asset (want $want, got $got)"
say "sha256 verified"

tar -xzf "$tmp/$asset" -C "$tmp" slopcop || die "archive does not contain slopcop"
install_binary "$tmp/slopcop"
