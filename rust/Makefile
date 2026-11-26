# The Rust core of Quassel.
#
# Every target does something on its own; `make help` lists them.

CARGO ?= cargo
DESTDIR ?=
PREFIX ?= /usr/local
BINDIR = $(DESTDIR)$(PREFIX)/bin
MANDIR = $(DESTDIR)$(PREFIX)/share/man/man1
CARGO_FLAGS ?=
CARGO_TARGET_DIR ?= target
FUZZ_DIR = crates/qs-datastream/fuzz
FUZZ_TARGET ?= datastream

.PHONY: help all build release test check fmt fmt-check clippy doc fuzz fuzz-build clean install uninstall

help:
	@echo "The Rust core of Quassel.  The targets:"
	@echo
	@echo "  build       build the workspace (debug)"
	@echo "  release     build the core (release)"
	@echo "  test        run the tests of the workspace"
	@echo "  check       check the formatting and run clippy"
	@echo "  fmt         format the sources"
	@echo "  fmt-check   check the formatting, and fail when it is off"
	@echo "  clippy      run clippy, and fail on a warning"
	@echo "  doc         build the documentation"
	@echo "  fuzz-build  build the fuzz target of the codec (needs cargo-fuzz)"
	@echo "  fuzz        run the fuzz target of the codec (needs cargo-fuzz)"
	@echo "  install     install the core and its manual page"
	@echo "  uninstall   remove what install put in place"
	@echo "  clean       remove the build directory"

all: build

build:
	$(CARGO) build --workspace $(CARGO_FLAGS)

release:
	$(CARGO) build --workspace --release $(CARGO_FLAGS)

test:
	$(CARGO) test --workspace $(CARGO_FLAGS)

check: fmt-check clippy

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

clippy:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

doc:
	$(CARGO) doc --workspace --no-deps

# The fuzz target is driven by cargo-fuzz: it needs a nightly toolchain and
# the cargo-fuzz tool, which install the sanitizer and the coverage flags
# the fuzz target needs.  A plain cargo build does not do that.
CARGO_FUZZ ?= cargo fuzz

fuzz-build:
	cd $(FUZZ_DIR) && $(CARGO_FUZZ) build $(FUZZ_TARGET)

fuzz: fuzz-build
	cd $(FUZZ_DIR) && $(CARGO_FUZZ) run $(FUZZ_TARGET) -- -max_total_time=60

install: release
	mkdir -p $(BINDIR) $(MANDIR)
	cp $(CARGO_TARGET_DIR)/release/quasselcore-rs $(BINDIR)/quasselcore-rs
	cp man/quasselcore-rs.1 $(MANDIR)/quasselcore-rs.1

uninstall:
	rm -f $(BINDIR)/quasselcore-rs
	rm -f $(MANDIR)/quasselcore-rs.1

clean:
	$(CARGO) clean
