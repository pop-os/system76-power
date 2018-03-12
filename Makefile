all: target/release/system76-power

clean:
	cargo clean

target/release/system76-power: Cargo.lock Cargo.toml src/*
	cargo build --release

install: target/release/system76-power
	install --mode="a+rx,u+ws" $< /usr/local/bin/system76-power

uninstall:
	rm -f /usr/local/bin/system76-power
