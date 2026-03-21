pkgname=late-thunderbolt
pkgver=0.1.3
pkgrel=1
pkgdesc="Load thunderbolt JIT for ZFS pools on a TBT3 JBOD"
arch=('x86_64')
license=('custom')
depends=()
makedepends=('cargo')
install=late-thunderbolt.install
source=()
sha256sums=()

build() {
	cargo build --release --locked
}

package() {
	cd $startdir

	# binary in user space which waits for the JBOD
	install -Dm754 target/release/init-wait-ahci \
		"$pkgdir/usr/local/bin/init-wait-ahci"

	chown root:wheel "$pkgdir/usr/local/bin/init-wait-ahci"

	# systemd unit to call that binary after switch_root
	install -Dm 644 dist/units/late-thunderbolt.service \
		"$pkgdir/usr/lib/systemd/system/late-thunderbolt.service"

	# initcpio hook to remove the JBOD from the PCIe topology before fs mount
	install -Dm754 dist/hooks/rm-tb-pci \
		"$pkgdir/etc/initcpio/hooks/rm-tb-pci"

	install -Dm754 dist/install/sd-tb-pci \
		"$pkgdir/etc/initcpio/install/sd-tb-pci"

	install -Dm644 dist/units/rm-tb.service \
		"$pkgdir/etc/initcpio/units/rm-tb.service"
}
