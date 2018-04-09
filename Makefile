prefix ?= /usr
sysconfdir ?= /etc
exec_prefix = $(prefix)
bindir = $(exec_prefix)/bin
libdir = $(exec_prefix)/lib
includedir = $(prefix)/include
datarootdir = $(prefix)/share
datadir = $(datarootdir)

.PHONY: all clean distclean install uninstall update

BIN=system76-power

all: target/release/$(BIN)

clean:
	cargo clean

distclean: clean
	rm -rf .cargo vendor

install: all
	install -D -m 04755 "target/release/$(BIN)" "$(DESTDIR)$(bindir)/$(BIN)"
	install -D -m 0644 "data/$(BIN).conf" "$(DESTDIR)$(sysconfdir)/dbus-1/system.d/$(BIN).conf"

uninstall:
	rm -f "$(DESTDIR)$(bindir)/$(BIN)"
	rm -f "$(DESTDIR)$(sysconfdir)/dbus-1/system.d/$(BIN).conf"

update:
	cargo update

.cargo/config: vendor_config
	mkdir -p .cargo
	cp $< $@

vendor: .cargo/config
	cargo vendor
	touch vendor

target/release/$(BIN):
	if [ -d vendor ]; \
	then \
		cargo build --release --frozen; \
	else \
		cargo build --release; \
	fi
