# Maintainer: JaINTP <jaintp@example.com>
pkgname=auror-git
_pkgname=auror
pkgver=0.1.0.r0.g0000000
pkgrel=1
pkgdesc="Arch Linux AUR local repository background daemon (aurord) and TUI dashboard (aurorc)"
arch=('x86_64' 'aarch64')
url="https://github.com/JaINTP/auror"
license=('MIT')
depends=('gcc-libs' 'glibc')
makedepends=('cargo' 'git')
provides=('auror' 'aurord' 'aurorc')
conflicts=('aur-updater' 'aurd' 'aurc' 'auror' 'aurord' 'aurorc')
source=("$_pkgname::git+file://$STARTDIR")
sha256sums=('SKIP')
options=(!lto)

pkgver() {
  cd "$srcdir/$_pkgname"
  printf "0.1.0.r%s.%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

prepare() {
  cd "$srcdir/$_pkgname"
  export CARGO_HOME="$srcdir/cargo"
  cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}

build() {
  cd "$srcdir/$_pkgname"
  export CARGO_HOME="$srcdir/cargo"
  cargo build --release --frozen
}

package() {
  cd "$srcdir/$_pkgname"

  # Install binaries
  install -Dm755 target/release/aurord "$pkgdir/usr/bin/aurord"
  install -Dm755 target/release/aurorc "$pkgdir/usr/bin/aurorc"

  # Install systemd user service
  install -Dm644 aurord.service "$pkgdir/usr/lib/systemd/user/aurord.service"

  # Adjust ExecStart in the installed systemd service to point to the global /usr/bin/aurord
  sed -i 's|ExecStart=.*|ExecStart=/usr/bin/aurord|' "$pkgdir/usr/lib/systemd/user/aurord.service"
}
