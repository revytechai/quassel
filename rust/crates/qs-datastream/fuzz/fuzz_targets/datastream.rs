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

//! Fuzz target for the Quassel DataStream decoder.
//!
//! The decoder reads bytes that came from the network, so it must never
//! panic, never loop forever and never allocate an absurd amount of memory.
//! Whatever it accepts, it must also be able to write back out.
//!
//! The `inputs` directory holds the seeds that must never crash the
//! decoder: a value nested a thousand levels deep, a string with an odd
//! byte count, a string with an unpaired surrogate and a map with a null
//! key.  Replay them with
//!
//! ```text
//! cargo fuzz run datastream crates/qs-datastream/fuzz/inputs
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;
use qs_datastream::{decode_list_body, decode_value, encode_list_body_to_vec, encode_value_to_vec};

fuzz_target!(|data: &[u8]| {
    // A whole value.
    if let Ok((value, used)) = decode_value(data) {
        // The decoder must not read past the end of the buffer.
        assert!(used <= data.len());
        // What comes out must go back in, and the bytes must not change.
        let encoded = encode_value_to_vec(&value);
        let (again, used) = decode_value(&encoded).expect("re-decoding our own output must work");
        assert_eq!(again, value);
        assert_eq!(used, encoded.len());
    }

    // A message body.
    if let Ok((values, used)) = decode_list_body(data) {
        assert!(used <= data.len());
        let encoded = encode_list_body_to_vec(&values);
        let (again, used) = decode_list_body(&encoded).expect("re-decoding our own output must work");
        assert_eq!(again, values);
        assert_eq!(used, encoded.len());
    }
});
