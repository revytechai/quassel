# SPDX-License-Identifier: BSD-2-Clause
# Copyright (c) 2026 REVYTECH, Inc.
#
# Quassel IRC - root Makefile for the revytechai/quassel fork.
#
# This is a thin fork wrapper: it follows upstream's build and adds nothing of
# its own. Upstream (quassel/quassel) builds with CMake, so UPSTREAM_BUILD is
# cmake and every target below only drives upstream's own build system.
#
# Portability (GNU make and FreeBSD/OpenBSD bmake):
#   - recipe lines start with a real tab; no .RECIPEPREFIX;
#   - no .ONESHELL: one shell per recipe line, chained with && or a backslash;
#   - no ifeq/.if: the OS is detected inside the recipe with uname and a case;
#   - plain = assignments, $(VAR), $$ for the shell;
#   - no include/.include, no GNU functions, no bmake modifiers, no $^ or $>;
#   - help is the first target (.DEFAULT_GOAL is GNU-only);
#   - no knob is spliced into a recipe: the knobs are read from the
#     environment, which both makes hand to a recipe unchanged;
#   - every command that may fail carries its own handler: bmake runs a
#     recipe with "sh -e" and GNU make does not, so a recipe that leans on
#     ";" to carry on would stop half way under bmake.
# See skill project-makefile-contract.
#
# The shell work the recipes share lives in scripts/make-common.sh, which is
# sourced by the recipes that need it.

REPO_PURPOSE = fork

# A fork follows upstream's build: upstream quassel builds with CMake.
UPSTREAM_BUILD = cmake
UPSTREAM_URL = https://github.com/quassel/quassel.git

# The knobs. They are read from the environment by the recipes, and are not
# assigned here on purpose: a makefile assignment would overwrite the value
# the caller gave in the environment, and the recipes would then see the
# makefile's default instead of the caller's. Both GNU make and bmake export
# a command line variable to the recipe environment, so a knob is set the
# same way on every platform:
#
#	make build BUILD_DIR=/somewhere/else
#	make build BUILD_TYPE=Debug
#	make build TESTS=on
#	JSON=1 make preflight
#
# The defaults, and what each one means, are in scripts/make-common.sh:
#
#	BUILD_DIR	empty: a build directory of this checkout's own, so that
#			two checkouts side by side never share one
#			cmake cache
#	BUILD_TYPE	Release
#	WANT_CLIENT	OFF: the IRC core, and nothing else
#	CMAKE_OPTIONS	-DWANT_MONO=OFF -DWITH_KDE=OFF -DWITH_WEBENGINE=OFF
#			-DWITH_LDAP=OFF; only -DNAME=ON and -DNAME=OFF are
#			accepted, so nothing else can reach the cmake
#			command line
#	TESTS		off; on builds upstream's unit tests, which need the
#			GUI client target, and turns the client on
#	JSON		0; 1 makes preflight write the --json contract

# help is the first target, so a bare "make" prints usage on both makes.
help:
	@printf '%s\n' \
		"purpose: $(REPO_PURPOSE)" \
		"" \
		"Quassel IRC - thin fork Makefile; it follows upstream's build." \
		"  upstream build system: $(UPSTREAM_BUILD)  ($(UPSTREAM_URL))" \
		"" \
		"Targets (same names on every OS):" \
		"  make help          this message (default)" \
		"  make purpose       print REPO_PURPOSE (for machines)" \
		"  make preflight     report the missing build dependencies (JSON=1 for the --json contract)" \
		"  make build         configure and build the core out of tree" \
		"  make test          run ctest in the build directory" \
		"  make check         run this Makefile's own tests" \
		"" \
		"Switches (on the command line, or in the environment):" \
		"  BUILD_DIR=<dir>      out-of-tree build directory (default: one for this checkout)" \
		"  BUILD_TYPE=<type>    cmake build type (default: Release)" \
		"  WANT_CLIENT=on|off   build the GUI client too (default: off)" \
		"  TESTS=on|off         build upstream's unit tests (needs the client)" \
		"  JSON=1               preflight writes the --json contract" \
		"  CMAKE_OPTIONS=...    extra cmake options; only -DNAME=ON and -DNAME=OFF" \
		"" \
		"Run make from the directory that holds this Makefile."

.PHONY: help purpose preflight build test check

purpose:
	@printf '%s\n' "$(REPO_PURPOSE)"

# preflight reports the build dependencies of the Qt the tree builds
# against: cmake itself, the qt development files (the cmake package of the
# Qt core) and the Qt SQL driver plugin (libqsqlite) the core loads at run
# time. The Qt version is the one the tree's own CMakeLists.txt asks for.
# JSON=1 writes the --json contract: metadata, an indexed status array and
# the purpose. It exits 0 when everything is present, 1 when something is
# missing, and it still writes the contract when it fails.
preflight:
	@# JSON=1 writes the --json contract. The switch comes from the
	@# environment, like every other knob: no make variable is spliced
	@# into a recipe.
	@if [ ! -r ./scripts/make-common.sh ]; then \
		printf 'preflight: scripts/make-common.sh is missing; run make from the directory that holds this Makefile\n' >&2; \
		exit 1; \
	fi; \
	. ./scripts/make-common.sh || exit 1; \
	mk_apply_defaults; \
	os=`uname -s`; arch=`uname -m`; cwd=`pwd -P`; \
	purpose=`mk_repo_purpose`; \
	if [ -z "$$purpose" ]; then \
		printf 'preflight: cannot read REPO_PURPOSE from %s; run make from the directory that holds this Makefile\n' "$MK_MAKEFILE" >&2; \
		exit 1; \
	fi; \
	human=; jstatus=; idx=0; missing=0; \
	add() { \
		_name="$$1"; _state="$$2"; _info="$$3"; \
		_en=`mk_json_escape "$$_name"`; _ei=`mk_json_escape "$$_info"`; \
		if [ "$$_state" = ok ]; then \
			human=`printf '%s\n%s' "$$human" "ok    $$_name ($$_info)"`; \
			jstatus=`printf '%s\n%s' "$$jstatus" "{\"index\": $$idx, \"name\": \"$$_en\", \"state\": \"ok\", \"available\": true}"`; \
		else \
			missing=`expr $$missing + 1`; \
			human=`printf '%s\n%s' "$$human" "miss  $$_name ($$_info)"`; \
			jstatus=`printf '%s\n%s' "$$jstatus" "{\"index\": $$idx, \"name\": \"$$_en\", \"state\": \"missing\", \"available\": false, \"reason\": \"$$_ei\"}"`; \
		fi; \
		idx=`expr $$idx + 1`; \
	}; \
	cmver=`mk_cmake_version`; \
	if [ -z "$$cmver" ]; then \
		add cmake missing "cmake is not on PATH"; \
	elif [ "`mk_version_ge "$$cmver" 3.13`" != yes ]; then \
		add cmake missing "cmake $$cmver is too old; this build needs 3.13 or newer"; \
	else \
		add cmake ok "$$cmver (`command -v cmake`)"; \
	fi; \
	qtmajor=`mk_qt_major`; \
	qtcoredir=`mk_find_qt_core`; \
	if [ -n "$$qtcoredir" ]; then \
		add "qt$$qtmajor-base" ok "$$qtcoredir"; \
	else \
		add "qt$$qtmajor-base" missing "no Qt$$qtmajor Core cmake package under `mk_qt_prefixes | tr '\n' ' '`"; \
	fi; \
	sqldriver=`mk_find_sqlite_driver`; \
	if [ -n "$$sqldriver" ]; then \
		add sqlite-driver ok "$$sqldriver"; \
	else \
		add sqlite-driver missing "no Qt SQL driver plugin libqsqlite under `mk_qt_prefixes | tr '\n' ' '`"; \
	fi; \
	if [ "$$JSON" = 1 ]; then \
		_p=`mk_json_escape "$$purpose"`; \
		_os=`mk_json_escape "$$os"`; \
		_arch=`mk_json_escape "$$arch"`; \
		_cwd=`mk_json_escape "$$cwd"`; \
		printf '{\n'; \
		printf '  "schema_version": "1.0",\n'; \
		printf '  "tool": "preflight",\n'; \
		printf '  "purpose": "%s",\n' "$$_p"; \
		printf '  "metadata": {\n'; \
		printf '    "os": "%s",\n' "$$_os"; \
		printf '    "arch": "%s",\n' "$$_arch"; \
		printf '    "cwd": "%s",\n' "$$_cwd"; \
		printf '    "checked": %s\n' "$$idx"; \
		printf '  },\n'; \
		printf '  "status": [\n'; \
		printf '%s\n' "$$jstatus" | sed -e '/^$$/d' -e 's/^/    /' -e 's/$$/,/' | sed -e '$$s/,$$//'; \
		printf '  ],\n'; \
		printf '  "items": []\n'; \
		printf '}\n'; \
	else \
		printf 'preflight: purpose=%s os=%s arch=%s\n' "$$purpose" "$$os" "$$arch"; \
		printf '%s\n' "$$human" | sed '/^$$/d'; \
		if [ "$$missing" -eq 0 ]; then printf 'preflight: all %s item(s) present\n' "$$idx"; else printf 'preflight: %s item(s) missing\n' "$$missing"; fi; \
	fi; \
	if [ "$$missing" -eq 0 ]; then exit 0; fi; \
	exit 1

# build configures upstream's cmake build out of tree and builds the core.
# TESTS=on asks for upstream's unit tests; they need the GUI client target,
# so the client is built as well when it is not wanted otherwise.
build:
	@if [ ! -r ./scripts/make-common.sh ]; then \
		printf 'build: scripts/make-common.sh is missing; run make from the directory that holds this Makefile\n' >&2; \
		exit 1; \
	fi; \
	. ./scripts/make-common.sh || exit 1; \
	mk_apply_defaults; \
	t=`mk_bool "$$TESTS"` || { printf 'build: TESTS must be on or off (got "%s")\n' "$$TESTS" >&2; exit 2; }; \
	client=`mk_bool "$$WANT_CLIENT"` || { printf 'build: WANT_CLIENT must be on or off (got "%s")\n' "$$WANT_CLIENT" >&2; exit 2; }; \
	opts=`mk_cmake_options "$$CMAKE_OPTIONS"` || exit 2; \
	if [ "$$t" = on ] && [ "$$client" = off ]; then \
		client=on; \
		printf 'build: TESTS=on needs the GUI client target, so the client is built too\n'; \
	fi; \
	if [ "$$t" = on ]; then \
		testing=ON; \
		printf 'build: unit tests enabled (BUILD_TESTING=ON)\n'; \
	else \
		testing=OFF; \
		printf 'build: unit tests off (BUILD_TESTING=OFF); upstream unit tests link the GUI client, use TESTS=on\n'; \
	fi; \
	if [ "$$client" = on ]; then wantclient=ON; else wantclient=OFF; fi; \
	bd=`mk_build_dir`; \
	printf 'build: build directory: %s\n' "$$bd"; \
	mkdir -p "$$bd" && \
	cmake -S . -B "$$bd" -DCMAKE_BUILD_TYPE="$$BUILD_TYPE" $$opts -DWANT_QTCLIENT=$$wantclient -DBUILD_TESTING=$$testing && \
	cmake --build "$$bd" --parallel

# test runs upstream's ctest suite in the build directory. ctest reports "no
# tests found" as a success, so the number of tests it discovered is checked
# first: a build tree without tests must not pass as a green test run.
test:
	@if [ ! -r ./scripts/make-common.sh ]; then \
		printf 'test: scripts/make-common.sh is missing; run make from the directory that holds this Makefile\n' >&2; \
		exit 1; \
	fi; \
	. ./scripts/make-common.sh || exit 1; \
	mk_apply_defaults; \
	bd=`mk_build_dir`; \
	if [ ! -d "$$bd" ]; then \
		printf 'test: %s does not exist; run "make build" first\n' "$$bd" >&2; \
		exit 1; \
	fi; \
	if [ ! -f "$$bd/CTestTestfile.cmake" ]; then \
		printf 'test: %s has no ctest tree; upstream unit tests link the GUI client target, so build them with: make build TESTS=on\n' "$$bd" >&2; \
		exit 1; \
	fi; \
	n=`mk_ctest_count "$$bd"`; \
	if [ -z "$$n" ] || [ "$$n" -eq 0 ]; then \
		printf 'test: %s holds no tests: ctest found none, so there is nothing to run\n' "$$bd" >&2; \
		exit 1; \
	fi; \
	printf 'test: %s: %s test(s) to run\n' "$$bd" "$$n"; \
	cd "$$bd" && ctest --output-on-failure

# check runs this Makefile's own tests: the behaviour the recipes promise.
check:
	@sh ./tests/makefile/run.sh
