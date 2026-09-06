# Maintainer: Ozan Özdil <ozdil>
pkgname=omarchy-omaplayer
pkgver=1.1.0
pkgrel=1
pkgdesc="Universal multi-source music player and internet radio workstation for Omarchy written in 100% Rust"
arch=('x86_64')
url="https://github.com/ozdil/omarchy-omaplayer"
license=('MIT')
depends=('glibc' 'gcc-libs' 'mpv')
makedepends=('cargo' 'rust')

build() {
    cd "${startdir}"
    cargo build --release --locked
}

package() {
    cd "${startdir}"
    install -Dm755 "target/release/omaplayer-engine" "${pkgdir}/usr/bin/omaplayer-engine"
    install -Dm755 "target/release/omaplayer-dashboard" "${pkgdir}/usr/bin/omaplayer-dashboard"
    install -Dm755 "target/release/omaplayer-status" "${pkgdir}/usr/bin/omaplayer-status"
    install -Dm755 "target/release/omaplayer-engine" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaplayer/omaplayer-engine"
    install -Dm755 "target/release/omaplayer-dashboard" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaplayer/omaplayer-dashboard"
    install -Dm755 "target/release/omaplayer-status" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaplayer/omaplayer-status"
    install -Dm644 "manifest.json" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaplayer/manifest.json"
    install -Dm644 "Panel.qml" "${pkgdir}/usr/share/omarchy/plugins/ozdil.omaplayer/Panel.qml"
    install -Dm644 "README.md" "${pkgdir}/usr/share/doc/${pkgname}/README.md"
    install -Dm644 "LICENSE" "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
}
