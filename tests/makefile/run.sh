#!/bin/sh
# SPDX-License-Identifier: BSD-2-Clause
# Copyright (c) 2026 REVYTECH, Inc.
#
# run.sh -- the root Makefile's own tests.
#
# The Makefile promises behaviour that only shows up when the recipes run:
# that two checkouts do not share a build directory, that turning the tests
# off really turns them off, that a value from the command line cannot
# become a command, and that the JSON contract is JSON. Each of those is
# checked here, against the Makefile as it stands.
#
# The Makefile under test is the one in the repository, or the one in the
# directory given as the first argument. The tests run under every make that
# is installed, because the Makefile has to work under both.
#
# usage: sh tests/makefile/run.sh [directory holding the Makefile]
#
# The scratch directory is TMPDIR/quassel-makefile-test.XXXXXX; TMPDIR is
# honoured when it is set to something other than /tmp.

set -u

here=$(cd "$(dirname "$0")" && pwd -P)
SRC=$(cd "${1:-$here/../..}" && pwd -P)
[ -f "$SRC/Makefile" ] || { echo "run.sh: no Makefile in $SRC" >&2; exit 2; }

if [ -n "${TMPDIR:-}" ]; then
	case $TMPDIR in
	/tmp|/tmp/*) ;;
	*) base=${TMPDIR%/} ;;
	esac
fi
: "${base:=/workspace}"
[ -d "$base" ] && [ -w "$base" ] || base="${HOME%/}/work"
WORK=$(mktemp -d "$base/quassel-makefile-test.XXXXXX") || exit 2
trap 'rm -rf "$WORK"' EXIT INT TERM HUP

pass_n=0
fail_n=0

ok() {
	pass_n=$((pass_n + 1))
	printf 'ok   %s\n' "$1"
}

no() {
	fail_n=$((fail_n + 1))
	printf 'FAIL %s\n' "$1"
}

assert_eq() { # NAME WANT GOT
	if [ "$2" = "$3" ]; then
		ok "$1"
	else
		no "$1 (want [$2], got [$3])"
	fi
}

assert_grep() { # NAME FILE PATTERN
	if grep -q -- "$3" "$2" 2>/dev/null; then
		ok "$1"
	else
		no "$1 (no match for [$3] in $2)"
	fi
}

assert_not_grep() { # NAME FILE PATTERN
	if grep -q -- "$3" "$2" 2>/dev/null; then
		no "$1 (unexpected match for [$3] in $2)"
	else
		ok "$1"
	fi
}

assert_absent() { # NAME FILE
	if [ -e "$2" ]; then
		no "$1 ($2 exists)"
	else
		ok "$1"
	fi
}

# assert_grep_fixed: like assert_grep, but the pattern is a fixed string, so
# a path is not read as a regular expression.
assert_grep_fixed() { # NAME FILE PATTERN
	if grep -qF -- "$3" "$2" 2>/dev/null; then
		ok "$1"
	else
		no "$1 (no match for [$3] in $2)"
	fi
}

# mktree NAME: a directory with the Makefile under test and its helper
# script, and nothing else.
mktree() {
	d=$WORK/$1
	mkdir -p "$d"
	cp "$SRC/Makefile" "$d/Makefile"
	if [ -f "$SRC/scripts/make-common.sh" ]; then
		mkdir -p "$d/scripts"
		cp "$SRC/scripts/make-common.sh" "$d/scripts/make-common.sh"
	fi
	printf '%s\n' "$d"
}

# The stubs: a cmake, a ctest and a qmake6 that record what they were asked
# and answer with what the test wants them to answer.
mkdir -p "$WORK/bin"
cat > "$WORK/bin/cmake" <<'STUB'
#!/bin/sh
printf '%s\n' "$*" >> "$MK_CMAKE_LOG"
case "$1" in
--version) echo "cmake version ${MK_CMAKE_VERSION:-3.31.12}" ;;
esac
exit 0
STUB
cat > "$WORK/bin/ctest" <<'STUB'
#!/bin/sh
printf '%s\n' "$*" >> "$MK_CTEST_LOG"
case "$1" in
-N) echo "Total Tests: ${MK_CTEST_TESTS:-0}" ;;
esac
exit 0
STUB
cat > "$WORK/bin/qmake6" <<'STUB'
#!/bin/sh
case "$1" in
-query)
	case "$2" in
	QT_INSTALL_PREFIX) echo "QT_INSTALL_PREFIX:${MK_QT_PREFIX:-}" ;;
	QT_INSTALL_PLUGINS) echo "QT_INSTALL_PLUGINS:${MK_QT_PREFIX:-}/plugins" ;;
	esac
	;;
esac
exit 0
STUB
chmod +x "$WORK/bin/cmake" "$WORK/bin/ctest" "$WORK/bin/qmake6"

# run_make DIR LOGFILE ARGS...: run the make under test in DIR with the
# stubs on PATH. The output goes to $WORK/out.
#
# MAKELEVEL and MAKEFLAGS are cleared: a make that runs inside another make
# is a sub-make, and GNU make prints "Entering directory" for one, which
# would land in the output these tests read.
run_make() {
	dir=$1
	log=$2
	shift 2
	: > "$log"
	(cd "$dir" && MAKEFLAGS= MAKELEVEL= PATH="$WORK/bin:$PATH" \
	    MK_CMAKE_LOG="$log" MK_CTEST_LOG="$log" \
	    "$MAKE_UNDER_TEST" "$@" >"$WORK/out" 2>&1)
	rc=$?
	return $rc
}

# json_is NAME FILE EXPECTED_CWD: the file must be JSON, and its metadata
# must say exactly this working directory and nothing else.
json_is() {
	if ! command -v python3 >/dev/null 2>&1; then
		ok "$1 (skipped: no python3)"
		return 0
	fi
	if python3 - "$2" "$3" >"$WORK/json.err" 2>&1 <<'PY'
import json, sys
path, want = sys.argv[1], sys.argv[2]
try:
    with open(path, encoding="utf-8") as fh:
        doc = json.load(fh)
except Exception as exc:
    print("not JSON: %s" % exc)
    sys.exit(1)
meta = doc.get("metadata")
if not isinstance(meta, dict):
    print("no metadata object")
    sys.exit(1)
if meta.get("cwd") != want:
    print("metadata.cwd is %r, want %r" % (meta.get("cwd"), want))
    sys.exit(1)
extra = sorted(set(meta) - {"os", "arch", "cwd", "checked"})
if extra:
    print("metadata has extra keys: %s" % extra)
    sys.exit(1)
if doc.get("purpose") != "fork":
    print("purpose is %r" % doc.get("purpose"))
    sys.exit(1)
PY
	then
		ok "$1"
	else
		no "$1 ($(cat "$WORK/json.err"))"
	fi
}

# ---------------------------------------------------------------- cases ---

case_help() {
	t=$(mktree help)
	run_make "$t" "$WORK/help.log" help
	assert_eq "help: first line is purpose: fork" "purpose: fork" \
	    "$(sed -n '1p' "$WORK/out")"
	run_make "$t" "$WORK/help.log" purpose
	assert_eq "purpose: prints fork" "fork" "$(cat "$WORK/out")"
}

# Two checkouts side by side must not share a build directory: the second
# cmake configure would run against the first one's cache and fail.
case_build_dir_is_per_checkout() {
	a=$(mktree sibling-a)
	b=$(mktree sibling-b)
	: > "$WORK/sibling.log"
	(cd "$a" && MAKEFLAGS= MAKELEVEL= PATH="$WORK/bin:$PATH" \
	    MK_CMAKE_LOG="$WORK/sibling.log" \
	    "$MAKE_UNDER_TEST" build >/dev/null 2>&1)
	(cd "$b" && MAKEFLAGS= MAKELEVEL= PATH="$WORK/bin:$PATH" \
	    MK_CMAKE_LOG="$WORK/sibling.log" \
	    "$MAKE_UNDER_TEST" build >/dev/null 2>&1)
	ba=$(sed -n 's/.*-B \([^ ]*\).*/\1/p' "$WORK/sibling.log" | sed -n '1p')
	bb=$(sed -n 's/.*-B \([^ ]*\).*/\1/p' "$WORK/sibling.log" | sed -n '2p')
	if [ -n "$ba" ] && [ -n "$bb" ] && [ "$ba" != "$bb" ]; then
		ok "build: two checkouts get different build directories"
	else
		no "build: two checkouts get different build directories (got [$ba] and [$bb])"
	fi
}

# Turning the tests off must turn them off in the cmake cache too, or a
# later build keeps the tests it was told not to build.
case_build_testing_is_explicit() {
	t=$(mktree testing)
	run_make "$t" "$WORK/testing-on.log" build TESTS=on
	assert_grep "build: TESTS=on configures BUILD_TESTING=ON" \
	    "$WORK/testing-on.log" '-DBUILD_TESTING=ON'
	run_make "$t" "$WORK/testing-off.log" build TESTS=off
	assert_grep "build: TESTS=off configures BUILD_TESTING=OFF" \
	    "$WORK/testing-off.log" '-DBUILD_TESTING=OFF'
}

# One spelling of a boolean, whatever its case, and no guessing.
case_booleans() {
	t=$(mktree booleans)
	run_make "$t" "$WORK/bool-true.log" build TESTS=TRUE
	assert_grep "build: TESTS=TRUE is on" "$WORK/bool-true.log" \
	    '-DBUILD_TESTING=ON'
	run_make "$t" "$WORK/bool-client.log" build TESTS=on WANT_CLIENT=FALSE
	assert_grep "build: TESTS=on forces the client on" \
	    "$WORK/bool-client.log" '-DWANT_QTCLIENT=ON'
	run_make "$t" "$WORK/bool-bad.log" build TESTS=bogus
	rc=$?
	if [ "$rc" -ne 0 ] && [ ! -s "$WORK/bool-bad.log" ]; then
		ok "build: TESTS=bogus is refused before anything is built"
	else
		no "build: TESTS=bogus is refused before anything is built (rc $rc)"
	fi
}

# A value from the command line is data, never a command.
case_no_shell_injection() {
	t=$(mktree injection)
	marker=$WORK/pwned
	rm -f "$marker"
	run_make "$t" "$WORK/inject-client.log" build \
	    "WANT_CLIENT=; touch $marker; exit 0"
	rc=$?
	assert_absent "build: a WANT_CLIENT with a command in it runs nothing" \
	    "$marker"
	if [ "$rc" -ne 0 ]; then
		ok "build: a WANT_CLIENT with a command in it is refused"
	else
		no "build: a WANT_CLIENT with a command in it is refused (rc $rc)"
	fi
	rm -f "$marker"
	run_make "$t" "$WORK/inject-opts.log" build \
	    "CMAKE_OPTIONS=-DWANT_MONO=OFF; touch $marker"
	rc=$?
	assert_absent "build: a CMAKE_OPTIONS with a command in it runs nothing" \
	    "$marker"
	if [ "$rc" -ne 0 ]; then
		ok "build: a CMAKE_OPTIONS with a command in it is refused"
	else
		no "build: a CMAKE_OPTIONS with a command in it is refused (rc $rc)"
	fi
	rm -f "$marker"
	run_make "$t" "$WORK/inject-opt2.log" build "CMAKE_OPTIONS=-DFOO=bar"
	rc=$?
	assert_absent "build: a CMAKE_OPTIONS that is not a boolean runs nothing" \
	    "$marker"
	if [ "$rc" -ne 0 ]; then
		ok "build: a CMAKE_OPTIONS that is not a boolean is refused"
	else
		no "build: a CMAKE_OPTIONS that is not a boolean is refused (rc $rc)"
	fi
}

# The JSON contract is JSON, whatever the working directory is called.
case_json_is_json() {
	d="$WORK/q\"repo\\slash"
	mkdir -p "$d"
	cp "$SRC/Makefile" "$d/Makefile"
	if [ -f "$SRC/scripts/make-common.sh" ]; then
		mkdir -p "$d/scripts"
		cp "$SRC/scripts/make-common.sh" "$d/scripts/make-common.sh"
	fi
	want=$(cd "$d" && pwd -P)
	run_make "$d" "$WORK/json.log" preflight JSON=1
	cp "$WORK/out" "$WORK/json.out"
	json_is "preflight: JSON=1 writes JSON with the cwd it ran in" \
	    "$WORK/json.out" "$want"
}

# A working directory that tries to add a property of its own does not.
case_json_has_no_injected_property() {
	d="$WORK/repo\", \"attacker\": \"yes"
	mkdir -p "$d"
	cp "$SRC/Makefile" "$d/Makefile"
	if [ -f "$SRC/scripts/make-common.sh" ]; then
		mkdir -p "$d/scripts"
		cp "$SRC/scripts/make-common.sh" "$d/scripts/make-common.sh"
	fi
	want=$(cd "$d" && pwd -P)
	run_make "$d" "$WORK/json2.log" preflight JSON=1
	cp "$WORK/out" "$WORK/json2.out"
	json_is "preflight: a directory name cannot add a property" \
	    "$WORK/json2.out" "$want"
	assert_not_grep "preflight: the contract has no attacker property" \
	    "$WORK/json2.out" -- attacker
}

# A cmake too old for the options this Makefile uses is not a cmake that
# can build this.
case_cmake_version_is_checked() {
	t=$(mktree cmake-version)
	MK_CMAKE_VERSION=3.12.0
	export MK_CMAKE_VERSION
	run_make "$t" "$WORK/cmake-old.log" preflight
	unset MK_CMAKE_VERSION
	cp "$WORK/out" "$WORK/cmake-old.out"
	assert_grep "preflight: cmake 3.12.0 is not reported as present" \
	    "$WORK/cmake-old.out" 'miss  cmake'
	assert_not_grep "preflight: cmake 3.12.0 is not reported as ok" \
	    "$WORK/cmake-old.out" 'ok    cmake'
	MK_CMAKE_VERSION=3.31.12
	export MK_CMAKE_VERSION
	run_make "$t" "$WORK/cmake-new.log" preflight
	unset MK_CMAKE_VERSION
	cp "$WORK/out" "$WORK/cmake-new.out"
	assert_grep "preflight: cmake 3.31.12 is reported as present" \
	    "$WORK/cmake-new.out" 'ok    cmake'
}

# The Qt files are found where they are actually installed.
case_qt_is_found_where_it_lives() {
	t=$(mktree qt)

	p=$WORK/qt-plain
	mkdir -p "$p/lib/cmake/Qt6Core"
	: > "$p/lib/cmake/Qt6Core/Qt6CoreConfig.cmake"
	MK_QT_PREFIX=$p
	MK_QT_PREFIXES=$p
	export MK_QT_PREFIX MK_QT_PREFIXES
	run_make "$t" "$WORK/qt1.log" preflight
	cp "$WORK/out" "$WORK/qt1.out"
	assert_grep "preflight: the Qt6Core package is found" "$WORK/qt1.out" \
	    'ok    qt6-base'
	# The reported directory must be the fixture's: a machine that has a Qt
	# of its own would otherwise satisfy this with its own installation and
	# the check could not fail.
	assert_grep_fixed "preflight: the Qt6Core package found is the fixture's" \
	    "$WORK/qt1.out" "$p/lib/cmake/Qt6Core"

	p=$WORK/qt-multiarch
	mkdir -p "$p/lib/x86_64-linux-gnu/cmake/Qt6Core"
	: > "$p/lib/x86_64-linux-gnu/cmake/Qt6Core/Qt6CoreConfig.cmake"
	MK_QT_PREFIX=$p
	MK_QT_PREFIXES=$p
	export MK_QT_PREFIX MK_QT_PREFIXES
	run_make "$t" "$WORK/qt2.log" preflight
	cp "$WORK/out" "$WORK/qt2.out"
	assert_grep "preflight: the Qt6Core package is found in a multiarch tree" \
	    "$WORK/qt2.out" 'ok    qt6-base'
	assert_grep_fixed \
	    "preflight: the multiarch Qt6Core package found is the fixture's" \
	    "$WORK/qt2.out" "$p/lib/x86_64-linux-gnu/cmake/Qt6Core"

	p=$WORK/qt-sql
	mkdir -p "$p/lib/x86_64-linux-gnu/qt6/plugins/sqldrivers"
	: > "$p/lib/x86_64-linux-gnu/qt6/plugins/sqldrivers/libqsqlite.so"
	MK_QT_PREFIX=$p
	MK_QT_PREFIXES=$p
	export MK_QT_PREFIX MK_QT_PREFIXES
	run_make "$t" "$WORK/qt3.log" preflight
	cp "$WORK/out" "$WORK/qt3.out"
	assert_grep "preflight: the SQL driver is found in a multiarch tree" \
	    "$WORK/qt3.out" 'ok    sqlite-driver'
	assert_grep_fixed \
	    "preflight: the multiarch SQL driver found is the fixture's" \
	    "$WORK/qt3.out" "$p/lib/x86_64-linux-gnu/qt6/plugins/sqldrivers/libqsqlite.so"

	# qmake6 on its own is not the development files.
	p=$WORK/qt-none
	mkdir -p "$p"
	MK_QT_PREFIX=$p
	MK_QT_PREFIXES=$p
	export MK_QT_PREFIX MK_QT_PREFIXES
	run_make "$t" "$WORK/qt4.log" preflight
	cp "$WORK/out" "$WORK/qt4.out"
	assert_grep "preflight: qmake6 without the cmake package is missing" \
	    "$WORK/qt4.out" 'miss  qt6-base'
	assert_not_grep "preflight: qmake6 without the cmake package is not ok" \
	    "$WORK/qt4.out" 'ok    qt6-base'
	unset MK_QT_PREFIX MK_QT_PREFIXES
}

# The tree says which Qt it wants; the preflight must follow the tree, not
# a version hard-coded in the Makefile.
case_qt_version_follows_the_tree() {
	t=$(mktree qt5tree)
	cat > "$t/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.5)
set(QT_MIN_VERSION "5.5.0")
find_package(Qt5 ${QT_MIN_VERSION} REQUIRED COMPONENTS Core Network)
EOF
	p=$WORK/qt5
	mkdir -p "$p/lib/cmake/Qt5Core" "$p/lib/qt5/plugins/sqldrivers"
	: > "$p/lib/cmake/Qt5Core/Qt5CoreConfig.cmake"
	: > "$p/lib/qt5/plugins/sqldrivers/libqsqlite.so"
	MK_QT_PREFIX=$p
	MK_QT_PREFIXES=$p
	export MK_QT_PREFIX MK_QT_PREFIXES
	run_make "$t" "$WORK/qt5.log" preflight
	cp "$WORK/out" "$WORK/qt5.out"
	assert_grep "preflight: a Qt 5 tree is checked for Qt 5" "$WORK/qt5.out" \
	    'ok    qt5-base'
	assert_grep_fixed "preflight: the Qt5 package found is the fixture's" \
	    "$WORK/qt5.out" "$p/lib/cmake/Qt5Core"
	assert_grep "preflight: the Qt 5 SQL driver is found" "$WORK/qt5.out" \
	    'ok    sqlite-driver'
	assert_grep_fixed "preflight: the Qt5 SQL driver found is the fixture's" \
	    "$WORK/qt5.out" "$p/lib/qt5/plugins/sqldrivers/libqsqlite.so"
	assert_not_grep "preflight: a Qt 5 tree is not checked for Qt 6" \
	    "$WORK/qt5.out" 'qt6-base'
	unset MK_QT_PREFIX MK_QT_PREFIXES
}

# A build tree with no tests in it is not a green test run.
case_test_needs_tests() {
	t=$(mktree testtarget)
	b=$WORK/empty-build
	mkdir -p "$b"
	: > "$b/CTestTestfile.cmake"
	run_make "$t" "$WORK/test-empty.log" test BUILD_DIR="$b"
	rc=$?
	if [ "$rc" -ne 0 ]; then
		ok "test: a build tree with no tests fails"
	else
		no "test: a build tree with no tests fails (rc $rc)"
	fi
	assert_grep "test: a build tree with no tests says so" "$WORK/out" \
	    'no tests'

	b=$WORK/with-tests
	mkdir -p "$b"
	: > "$b/CTestTestfile.cmake"
	MK_CTEST_TESTS=2
	export MK_CTEST_TESTS
	run_make "$t" "$WORK/test-with.log" test BUILD_DIR="$b"
	unset MK_CTEST_TESTS
	rc=$?
	if [ "$rc" -eq 0 ]; then
		ok "test: a build tree with tests runs them"
	else
		no "test: a build tree with tests runs them (rc $rc)"
	fi
	assert_grep "test: ctest was run" "$WORK/test-with.log" 'output-on-failure'
}

# ----------------------------------------------------------------- run ----

MAKES=""
for c in bmake bsdmake; do
	command -v "$c" >/dev/null 2>&1 && MAKES="$MAKES $c"
done
if [ -z "$MAKES" ] && command -v make >/dev/null 2>&1; then
	if ! make --version 2>/dev/null | grep -q 'GNU Make'; then
		MAKES=" make"
	fi
fi
for c in gmake gnumake; do
	command -v "$c" >/dev/null 2>&1 && MAKES="$MAKES $c"
done
[ -n "$MAKES" ] || { echo "run.sh: no make found" >&2; exit 2; }

for MAKE_UNDER_TEST in $MAKES; do
	printf '== %s ==\n' "$MAKE_UNDER_TEST"
	case_help
	case_build_dir_is_per_checkout
	case_build_testing_is_explicit
	case_booleans
	case_no_shell_injection
	case_json_is_json
	case_json_has_no_injected_property
	case_cmake_version_is_checked
	case_qt_is_found_where_it_lives
	case_qt_version_follows_the_tree
	case_test_needs_tests
done

printf '\n%d passed, %d failed\n' "$pass_n" "$fail_n"
[ "$fail_n" -eq 0 ] || exit 1
exit 0
