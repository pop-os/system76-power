prefix ?= /usr
sysconfdir ?= /etc
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
libdir = $(exec_prefix)/lib
includedir = $(prefix)/include
datarootdir = $(prefix)/share
datadir = $(datarootdir)

SRC = Cargo.toml Cargo.lock Makefile $(shell find src -type f -wholename '*src/*.rs')

.PHONY: all clean distclean install uninstall update

BIN=system76-power

DEBUG ?= 0
ifeq ($(DEBUG),0)
	ARGS += "--release"
	TARGET = release
endif

VENDORED ?= 0
ifeq ($(VENDORED),1)
	ARGS += "--frozen"
endif

DAEMON_DEST = "$(DESTDIR)$(bindir)/$(BIN)"
DBUS_DEST = "$(DESTDIR)$(sysconfdir)/dbus-1/system.d/$(BIN).conf"
CONFIG_DEST = "$(DESTDIR)$(libdir)/$(BIN)/config.toml"

all: target/release/$(BIN)

clean:
	cargo clean

distclean:
	rm -rf .cargo vendor vendor.tar.xz

install:
	install -Dm 04755 "target/$(TARGET)/$(BIN)" "$(DAEMON_DEST)"
	install -Dm 0644 "data/$(BIN).conf" "$(DBUS_DEST)"
	install -Dm 0644 "data/config.toml" "$(CONFIG_DEST)"

uninstall:
	rm -f "$(DAEMON_DEST)"
	rm -f "$(DBUS_DEST)"
	rm -f "$(CONFIG_DEST)"

update:
	cargo update

vendor:
	mkdir -p .cargo
	cargo vendor | head -n -1 > .cargo/config
	echo 'directory = "vendor"' >> .cargo/config
	tar pcfJ vendor.tar.xz vendor
	rm -rf vendor

target/$(TARGET)/$(BIN): $(SRC)
ifeq ($(VENDORED),1)
	tar pxf vendor.tar.xz
endif
	cargo build $(ARGS)
