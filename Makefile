PREFIX ?= /usr/local
BINDIR := $(PREFIX)/bin

.PHONY: build run install uninstall clean

build:
	cargo build --release

run:
	cargo run --bin cursor-cli-wrapper

install:
	cargo install --locked --path .

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/cursor-cli-wrapper
	rm -f $(DESTDIR)$(BINDIR)/cursor-cli-wrapper-backend

clean:
	cargo clean
