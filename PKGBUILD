# Maintainer: Ozan Özdil <ozan@pm.me>
pkgname=omarchy-omaplayer
pkgver=1.2.0
pkgrel=1
pkgdesc="Universal multi-source audio and internet radio studio for Omarchy written in 100% Rust"
arch=('x86_64')
url="https://github.com/ozdil/omarchy-omaplayer"
license=('MIT')
depends=('glibc' 'gcc-libs' 'mpv')
makedepends=('cargo' 'rust')
source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
    cd "$pkgname-$pkgver"
    cargo build --release --locked
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 "target/release/omaplayer-engine" "${pkgdir}/usr/lib/omarchy/plugins/omaplayer/omaplayer-engine"
    install -Dm755 "target/release/omaplayer-dashboard" "${pkgdir}/usr/lib/omarchy/plugins/omaplayer/omaplayer-dashboard"
    install -Dm755 "target/release/omaplayer-status" "${pkgdir}/usr/lib/omarchy/plugins/omaplayer/omaplayer-status"
    install -Dm755 "target/release/omaplayer-gui" "${pkgdir}/usr/lib/omarchy/plugins/omaplayer/omaplayer-gui"
    install -Dm644 "manifest.json" "${pkgdir}/usr/lib/omarchy/plugins/omaplayer/manifest.json"
    install -Dm644 "Panel.qml" "${pkgdir}/usr/lib/omarchy/plugins/omaplayer/Panel.qml"
    install -Dm644 "README.md" "${pkgdir}/usr/share/doc/${pkgname}/README.md"
    install -Dm644 "LICENSE" "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
}
