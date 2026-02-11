PREFIX ?= /usr/local
BINDIR := $(PREFIX)/bin

.PHONY: build install uninstall clean

build:
	cargo build --release

install: build
	install -Dm755 target/release/cursor-cli-wrapper $(DESTDIR)$(BINDIR)/cursor-cli-wrapper
	install -Dm755 target/release/cursor-cli-wrapper-backend $(DESTDIR)$(BINDIR)/cursor-cli-wrapper-backend

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/cursor-cli-wrapper
	rm -f $(DESTDIR)$(BINDIR)/cursor-cli-wrapper-backend

clean:
	cargo clean
