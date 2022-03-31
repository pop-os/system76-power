prefix ?= /usr
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
libdir = $(exec_prefix)/lib
includedir = $(prefix)/include
datadir = $(prefix)/share

SRC = Cargo.toml Cargo.lock Makefile $(shell find src -type f -wholename '*src/*.rs')

.PHONY: all clean distclean install uninstall update

BIN=system76-power
ID=com.system76.PowerDaemon

DEBUG ?= 0
ifeq ($(DEBUG),0)
	ARGS += "--release"
	TARGET = release
endif

VENDOR ?= 0
ifeq ($(VENDOR),1)
	ARGS += "--frozen"
endif

all: target/release/$(BIN)

clean:
	cargo clean

distclean:
	rm -rf .cargo vendor vendor.tar.xz

install: all
	install -D -m 0644 "data/$(ID).conf" "$(DESTDIR)$(datadir)/dbus-1/system.d/$(ID).conf"
	install -D -m 0644 "data/$(ID).policy" "$(DESTDIR)$(datadir)/polkit-1/actions/$(ID).policy"
	install -D -m 0644 "data/$(ID).service" "$(DESTDIR)$(libdir)/systemd/system/$(ID).service"
	install -D -m 0644 "data/$(ID).xml" "$(DESTDIR)$(datadir)/dbus-1/interfaces/$(ID).xml"
	install -D -m 0755 "target/release/$(BIN)" "$(DESTDIR)$(bindir)/$(BIN)"

uninstall:
	rm -f "$(DESTDIR)$(bindir)/$(ID)"
	rm -f "$(DESTDIR)$(datadir)/dbus-1/interfaces/$(ID).xml"
	rm -f "$(DESTDIR)$(datadir)/dbus-1/system.d/$(ID).conf"
	rm -f "$(DESTDIR)$(datadir)/polkit-1/actions/$(ID).policy"
	rm -f "$(DESTDIR)$(libdir)/systemd/system/$(ID).service"

update:
	cargo update

vendor:
	mkdir -p .cargo
	cargo vendor | head -n -1 > .cargo/config
	echo 'directory = "vendor"' >> .cargo/config
	tar pcfJ vendor.tar.xz vendor
	rm -rf vendor

target/release/$(BIN): $(SRC)
ifeq ($(VENDOR),1)
	tar pxf vendor.tar.xz
endif
	cargo build $(ARGS)
