# Maintainer: Gaurav Atreya <allmanpride@gmail.com>
pkgname=onchange
pkgver=0.1.1
pkgrel=1
pkgdesc="Run commands on file changes"
arch=('x86_64')
license=('GPL3')
depends=('gcc-libs')
makedepends=('rust' 'cargo')

build() {
	cargo build --release
}

package() {
    cd "$srcdir"
    mkdir -p "$pkgdir/usr/bin"
    cp "../target/release/${pkgname}" "$pkgdir/usr/bin/${pkgname}"
}
