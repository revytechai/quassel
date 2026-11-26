// Copyright (C) 2026 by the Quassel Project and the REVYTECH Rust port authors
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program; if not, write to the Free Software Foundation,
// Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301, USA.

//! Tests for the values the decoder refuses.
//!
//! The decoder reads bytes that came from the network, so it has to say no
//! to the ones it cannot make sense of, and it has to say no without
//! running out of stack.

use std::path::Path;

use qs_datastream::{decode_value_exact, DecodeError, Value};

/// The bytes of a value: a type id, a null flag and a payload.
fn value_bytes(type_id: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&type_id.to_be_bytes());
    bytes.push(0);
    bytes.extend_from_slice(payload);
    bytes
}

/// The bytes of `count` nested variants, wrapped around an `int`.
fn nested_variants(count: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    for _ in 0..count {
        bytes.extend_from_slice(&138u32.to_be_bytes());
        bytes.push(0);
    }
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&1i32.to_be_bytes());
    bytes
}

/// The bytes of a `QString`: a byte count and the UTF-16BE payload.
fn qstring(length: u32, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&length.to_be_bytes());
    body.extend_from_slice(payload);
    value_bytes(10, &body)
}

#[test]
fn a_value_nested_within_the_limit_is_accepted() {
    let decoded = decode_value_exact(&nested_variants(64)).unwrap();
    assert!(matches!(decoded, Value::Variant(_)));
}

#[test]
fn a_deeply_nested_variant_is_rejected() {
    let error = decode_value_exact(&nested_variants(65)).unwrap_err();
    assert!(
        matches!(
            error,
            DecodeError::TooDeep {
                depth: 65,
                limit: 64
            }
        ),
        "got {error}"
    );
}

#[test]
fn a_deeply_nested_value_does_not_overflow_the_stack() {
    // A hundred thousand levels of nesting is half a megabyte of input and
    // must be refused, not recursed into.
    let error = decode_value_exact(&nested_variants(100_000)).unwrap_err();
    assert!(matches!(error, DecodeError::TooDeep { .. }), "got {error}");
}

#[test]
fn a_string_with_an_odd_byte_count_is_rejected() {
    // A `QString` of one byte is not a UTF-16 string: the trailing byte
    // would be dropped, and the string would change on the way in.
    let error = decode_value_exact(&qstring(1, &[0x41])).unwrap_err();
    assert_eq!(error, DecodeError::InvalidUtf16);
}

#[test]
fn a_string_with_an_unpaired_surrogate_is_rejected() {
    // A lone high surrogate is not a character; it must not be replaced by
    // U+FFFD behind the caller's back.
    let error = decode_value_exact(&qstring(2, &[0xd8, 0x00])).unwrap_err();
    assert_eq!(error, DecodeError::InvalidUtf16);
}

#[test]
fn a_null_map_key_is_rejected() {
    // A map with one entry whose key is the null string.
    let mut payload = Vec::new();
    payload.extend_from_slice(&1u32.to_be_bytes());
    payload.extend_from_slice(&u32::MAX.to_be_bytes());
    payload.extend_from_slice(&2u32.to_be_bytes());
    payload.push(0);
    payload.extend_from_slice(&7i32.to_be_bytes());

    let error = decode_value_exact(&value_bytes(8, &payload)).unwrap_err();
    assert_eq!(error, DecodeError::NullMapKey);
}

/// The seeds the fuzzer replays must be refused, not crash the decoder.
#[test]
fn the_fuzz_seeds_are_refused() {
    let seeds = Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz/inputs");
    for seed in [
        "deeply_nested_variants",
        "odd_length_utf16",
        "unpaired_surrogate",
        "null_map_key",
    ] {
        let bytes = std::fs::read(seeds.join(seed))
            .unwrap_or_else(|error| panic!("reading the seed {seed} failed: {error}"));
        assert!(
            decode_value_exact(&bytes).is_err(),
            "the seed {seed} was accepted"
        );
    }
}
