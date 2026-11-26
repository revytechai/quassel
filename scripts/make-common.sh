#!/bin/sh
# SPDX-License-Identifier: BSD-2-Clause
# Copyright (c) 2026 REVYTECH, Inc.
#
# make-common.sh -- the shell helpers the root Makefile's recipes use.
#
# The Makefile drives upstream's cmake build; each of its recipes is a POSIX
# shell script. This file holds the parts the recipes share, so that the
# Makefile stays readable and so that the awkward parts (the JSON escaper,
# the boolean parser, the Qt discovery) can be tested on their own.
#
# Two rules shape the code.
#
# 1. No knob is ever spliced into shell source. The knobs (BUILD_DIR,
#    BUILD_TYPE, WANT_CLIENT, CMAKE_OPTIONS, TESTS, JSON) reach the recipes
#    through the environment: GNU make and bmake both export a command line
#    variable to the recipe environment, so an override arrives as data:
#
#	make build BUILD_DIR=/somewhere/else
#	JSON=1 make preflight
#
#    The defaults below are the only place they are written down.
#
# 2. Every value that reaches the shell is checked before it is used.
#
# A third rule follows from the first: bmake runs a recipe with "sh -e" and
# GNU make does not, so a helper here may not fail with a nonzero status
# unless the caller handles it. The helpers that look for something print
# nothing when it is not there, and say so with an empty result.
#
# usage: . ./scripts/make-common.sh, from the directory holding the Makefile.

# The Makefile the helpers read the repository's constants from.
MK_MAKEFILE=${MK_MAKEFILE:-Makefile}

# mk_makefile_var NAME: print the value NAME has in the Makefile. The
# Makefile is the single source of truth for the repository's constants, and
# reading them here keeps them out of the shell source of a recipe. Nothing
# is printed when the Makefile cannot be read.
mk_makefile_var() {
	if [ -r "$MK_MAKEFILE" ]; then
		sed -n "s/^$1[[:space:]]*=[[:space:]]*//p" "$MK_MAKEFILE" | head -1
	fi
}

# mk_repo_purpose: print the repository purpose. An environment override
# wins; otherwise the Makefile's declaration is used.
mk_repo_purpose() {
	if [ -n "${REPO_PURPOSE:-}" ]; then
		printf '%s\n' "$REPO_PURPOSE"
		return 0
	fi
	mk_makefile_var REPO_PURPOSE
}

# mk_apply_defaults: fill in the knobs that were not given, and export them,
# so that every recipe sees the same configuration.
#
#	BUILD_DIR	empty: a directory derived from this checkout's own path
#	BUILD_TYPE	Release
#	WANT_CLIENT	OFF: the IRC core, and nothing else
#	CMAKE_OPTIONS	upstream's features this fork does not want
#	TESTS		off
#	JSON		0
mk_apply_defaults() {
	: "${BUILD_TYPE:=Release}"
	: "${WANT_CLIENT:=OFF}"
	: "${CMAKE_OPTIONS:=-DWANT_MONO=OFF -DWITH_KDE=OFF -DWITH_WEBENGINE=OFF -DWITH_LDAP=OFF}"
	: "${TESTS:=off}"
	: "${JSON:=0}"
	: "${BUILD_DIR:=}"
	export BUILD_DIR BUILD_TYPE WANT_CLIENT CMAKE_OPTIONS TESTS JSON
}

# mk_bool VALUE: print "on" or "off" for VALUE, whatever its case. Anything
# else is not a boolean, and the caller must stop rather than guess.
mk_bool() {
	case "$(printf '%s' "$1" | tr 'A-Z' 'a-z')" in
	on|1|yes|true)	printf 'on' ;;
	off|0|no|false)	printf 'off' ;;
	*)		return 1 ;;
	esac
}

# mk_json_escape VALUE: print VALUE with the characters JSON forbids inside
# a string escaped: the quote, the backslash, the newline and the other
# control characters. The result is the body of a JSON string, without the
# quotes around it.
mk_json_escape() {
	printf '%s' "$1" | tr '\n' '\001' | awk '
BEGIN { ctrl = ""; for (i = 1; i < 32; i++) ctrl = ctrl sprintf("%c", i) }
function jesc(s,   i, c, out, p) {
	out = ""
	for (i = 1; i <= length(s); i++) {
		c = substr(s, i, 1)
		if (c == "\\") out = out "\\\\"
		else if (c == "\"") out = out "\\\""
		else if (c == "\001") out = out "\\n"
		else if (c == "\t") out = out "\\t"
		else if (c == "\r") out = out "\\r"
		else if (c == "\b") out = out "\\b"
		else if (c == "\f") out = out "\\f"
		else {
			p = index(ctrl, c)
			if (p > 0) out = out sprintf("\\u%04x", p - 1)
			else out = out c
		}
	}
	return out
}
{ printf "%s", jesc($0) }'
}

# mk_build_dir: print the build directory. BUILD_DIR wins when it is set.
# Otherwise the build goes to a directory that belongs to this checkout
# alone: two checkouts side by side would otherwise share one cmake cache,
# and the second configure would fail on the first one's source directory.
mk_build_dir() {
	if [ -n "${BUILD_DIR:-}" ]; then
		printf '%s\n' "$BUILD_DIR"
		return 0
	fi
	src=$(pwd -P)
	sum=$(printf '%s' "$src" | cksum | cut -d ' ' -f 1)
	printf '%s\n' "$(dirname "$src")/build-$(basename "$src")-$sum"
}

# mk_cmake_version: print the version of the cmake in PATH. Nothing is
# printed when there is no cmake.
mk_cmake_version() {
	if command -v cmake >/dev/null 2>&1; then
		cmake --version 2>/dev/null | awk 'NR == 1 { print $3; exit }'
	fi
}

# mk_version_ge HAVE WANT: print yes when HAVE is at least WANT.
mk_version_ge() {
	awk -v have="$1" -v want="$2" 'BEGIN {
		n = split(have, h, ".")
		m = split(want, w, ".")
		for (i = 1; i <= 3; i++) {
			a = (i <= n) ? h[i] + 0 : 0
			b = (i <= m) ? w[i] + 0 : 0
			if (a > b) { print "yes"; exit }
			if (a < b) { print "no"; exit }
		}
		print "yes"
	}'
}

# mk_qt_prefixes: print the prefixes to look for Qt 6 in, one per line. The
# prefix qmake6 reports comes first: it is the Qt that is actually
# installed. MK_QT_PREFIXES replaces the list, which is how the tests point
# the search at a fixture.
mk_qt_prefixes() {
	if command -v qmake6 >/dev/null 2>&1; then
		p=$(qmake6 -query QT_INSTALL_PREFIX 2>/dev/null |
		    sed -n 's/^QT_INSTALL_PREFIX://p')
		if [ -n "$p" ]; then
			printf '%s\n' "$p"
		fi
	fi
	if [ -n "${MK_QT_PREFIXES+set}" ]; then
		printf '%s\n' "$MK_QT_PREFIXES"
		return 0
	fi
	printf '%s\n' /usr/local /usr /opt/local /opt/homebrew
}

# mk_qt_major: print the Qt major version the tree builds against. It is
# read from the tree's own CMakeLists.txt, so the preflight cannot disagree
# with the build about which Qt is wanted. Six is the fallback when the tree
# says nothing.
mk_qt_major() {
	major=6
	f=${MK_CMAKE_LISTS:-CMakeLists.txt}
	if [ -r "$f" ]; then
		v=`sed -n 's/.*find_package(\(Qt[0-9]\)[^)]*REQUIRED.*/\1/p' "$f" | head -1`
		if [ -z "$v" ]; then
			v=`sed -n 's/.*find_package(\(Qt[0-9]\).*/\1/p' "$f" | head -1`
		fi
		case "$v" in
		Qt5) major=5 ;;
		Qt6) major=6 ;;
		esac
	fi
	printf '%s\n' "$major"
}

# mk_find_qt_core: print the directory that holds the cmake package of the
# Qt the tree builds against (QtNCoreConfig.cmake), the file that says the
# Qt development files are installed. qmake6 alone does not count: the
# build needs the cmake package, not the executable. Nothing is printed when
# there is none.
mk_find_qt_core() {
	major=`mk_qt_major`
	for p in $(mk_qt_prefixes); do
		for d in "$p/lib/cmake/Qt${major}Core" "$p/lib64/cmake/Qt${major}Core" \
		    "$p/lib/"*/cmake/Qt${major}Core "$p/share/cmake/Qt${major}Core" \
		    "$p/lib/qt${major}/cmake/Qt${major}Core"; do
			if [ -f "$d/Qt${major}CoreConfig.cmake" ]; then
				printf '%s\n' "$d"
				return 0
			fi
		done
	done
	return 0
}

# mk_find_sqlite_driver: print the Qt SQL driver plugin (libqsqlite) that
# the core loads at run time. The driver belongs to the Qt the tree builds
# against, so that is the one that is looked for: the plugin of another Qt
# cannot be loaded by this build. Nothing is printed when there is none.
mk_find_sqlite_driver() {
	major=`mk_qt_major`
	dirs=""
	if command -v qmake6 >/dev/null 2>&1; then
		qv=`qmake6 -query QT_VERSION 2>/dev/null |
		    sed -n 's/^QT_VERSION://p' | cut -d '.' -f 1`
		pl=`qmake6 -query QT_INSTALL_PLUGINS 2>/dev/null |
		    sed -n 's/^QT_INSTALL_PLUGINS://p'`
		if [ -n "$pl" ] && [ "$qv" = "$major" ]; then
			dirs="$pl/sqldrivers"
		fi
	fi
	for p in $(mk_qt_prefixes); do
		dirs="$dirs $p/lib/qt${major}/plugins/sqldrivers"
		dirs="$dirs $p/lib64/qt${major}/plugins/sqldrivers"
		dirs="$dirs $p/share/qt/plugins/sqldrivers"
		dirs="$dirs $p/plugins/sqldrivers"
		dirs="$dirs $p/qt${major}/plugins/sqldrivers"
		dirs="$dirs $p/lib/"*/qt${major}/plugins/sqldrivers
	done
	for d in $dirs; do
		for f in "$d"/libqsqlite.*; do
			if [ -f "$f" ]; then
				printf '%s\n' "$f"
				return 0
			fi
		done
	done
	return 0
}

# mk_cmake_options: print the CMake options to pass on, after checking every
# token. Only -DNAME=ON and -DNAME=OFF are allowed: a free-form option list
# would let a caller put anything into the cmake command line, and a token
# that survives this check cannot be anything else.
#
# The tokens are split with awk, which has its own field separator: setting
# IFS in the shell would change how every unquoted expansion in the recipes
# splits (semgrep: bash.lang.security.ifs-tampering).
mk_cmake_options() {
	printf '%s\n' "$1" | awk '
{
	for (i = 1; i <= NF; i++) {
		tok = $i
		n++
		toklist[n] = tok
		if (tok !~ /^-D[A-Za-z_][A-Za-z0-9_]*=(ON|OFF)$/) {
			printf "cmake option \"%s\" is not allowed: only -DNAME=ON and -DNAME=OFF are\n", tok > "/dev/stderr"
			bad = 1
		}
	}
}
END {
	if (bad) exit 1
	out = ""
	for (i = 1; i <= n; i++) out = out (i > 1 ? " " : "") toklist[i]
	printf "%s", out
}'
}

# mk_ctest_count DIR: print the number of tests ctest finds in DIR, zero
# when there are none. ctest reports "no tests found" as a success, so the
# count is what says whether there is anything to run.
mk_ctest_count() {
	(cd "$1" && ctest -N 2>/dev/null) | sed -n 's/^Total Tests: *//p' |
	    tr -d ' ' | head -1
}
