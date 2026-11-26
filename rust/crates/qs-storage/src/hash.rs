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

/*!
Password hashing.

The core stores a hash that says which scheme was used, so that a password
that was written by an older core can still be checked, and is rewritten
with the scheme of today on the first successful login.

The scheme of today is PBKDF2 with HMAC-SHA-512 (RFC 2898), a random salt
and a work factor of [`PBKDF2_ITERATIONS`].  One hash of a password is slow
on purpose: a database that leaks does not give the passwords away with a
single hash.

The two older schemes are only read:

* version 0 is the SHA-1 hash without a salt that the core used before 0.13,
* version 1 is the salted SHA-512 the core used before this scheme.
*/

use sha2::{Digest, Sha512};

use crate::error::StorageError;

/// The old hash version, SHA-1 without a salt.
pub const HASH_VERSION_SHA1: i64 = 0;
/// The salted SHA-512 the core used before the key derivation function.
pub const HASH_VERSION_SHA2_512: i64 = 1;
/// The hash version the core writes today.
pub const HASH_VERSION_PBKDF2: i64 = 2;
/// The hash version the core writes today.
pub const HASH_VERSION_CURRENT: i64 = HASH_VERSION_PBKDF2;

/// The name of the key derivation function, as it is written in the hash.
pub const PBKDF2_NAME: &str = "pbkdf2-sha512";

/// How many times the key derivation function is run.
pub const PBKDF2_ITERATIONS: u32 = 100_000;

/// The most iterations a stored hash may ask for.
///
/// The number of iterations is part of the stored hash, so that the work
/// factor can be raised later.  A broken or hostile database must not be
/// able to make the core spend its time on it.
pub const PBKDF2_MAX_ITERATIONS: u32 = 1_000_000;

/// How many random bytes the salt holds.
const SALT_BYTES: usize = 32;

/// Hashes a password the way the core does.
pub fn hash_password(password: &str) -> Result<(String, i64), StorageError> {
    let salt = random_salt()?;
    Ok((hash_with_salt(password, &salt), HASH_VERSION_PBKDF2))
}

/// Hashes a password with an existing salt.
pub fn hash_with_salt(password: &str, salt: &str) -> String {
    let derived = pbkdf2(password.as_bytes(), salt.as_bytes(), PBKDF2_ITERATIONS, 64);
    format!("{PBKDF2_NAME}${PBKDF2_ITERATIONS}${salt}${}", hex(&derived))
}

/// Checks a password against a stored hash.
pub fn check_password(password: &str, hashed: &str, hash_version: i64) -> bool {
    match hash_version {
        HASH_VERSION_SHA1 => hex(&sha1(password.as_bytes())) == hashed,
        HASH_VERSION_SHA2_512 => {
            let Some((_, salt)) = hashed.split_once(':') else {
                return false;
            };
            sha512_with_salt(password, salt) == hashed
        }
        HASH_VERSION_PBKDF2 => check_pbkdf2(password, hashed),
        _ => false,
    }
}

/// The salted SHA-512 the core used before the key derivation function.
fn sha512_with_salt(password: &str, salt: &str) -> String {
    let mut hasher = Sha512::new();
    hasher.update(password.as_bytes());
    hasher.update(salt.as_bytes());
    format!("{}:{}", hex(&hasher.finalize()), salt)
}

/// Checks a password against a hash of the key derivation function.
fn check_pbkdf2(password: &str, hashed: &str) -> bool {
    let mut parts = hashed.split('$');
    if parts.next() != Some(PBKDF2_NAME) {
        return false;
    }
    let Some(iterations) = parts.next().and_then(|text| text.parse::<u32>().ok()) else {
        return false;
    };
    if iterations == 0 || iterations > PBKDF2_MAX_ITERATIONS {
        return false;
    }
    let Some(salt) = parts.next() else {
        return false;
    };
    let Some(expected) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    let derived = pbkdf2(password.as_bytes(), salt.as_bytes(), iterations, 64);
    equal(hex(&derived).as_bytes(), expected.as_bytes())
}

/// Compares two byte strings without telling how much of them is the same.
fn equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in left.iter().zip(right.iter()) {
        difference |= left ^ right;
    }
    difference == 0
}

/// The key derivation function of RFC 2898.
fn pbkdf2(password: &[u8], salt: &[u8], iterations: u32, length: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(length);
    let mut block = 1u32;
    while output.len() < length {
        let mut previous = hmac_sha512(password, &block_of(salt, block));
        let mut result = previous;
        for _ in 1..iterations {
            previous = hmac_sha512(password, &previous);
            for (index, byte) in previous.iter().enumerate() {
                result[index] ^= byte;
            }
        }
        output.extend_from_slice(&result);
        block += 1;
    }
    output.truncate(length);
    output
}

/// The salt of one block of the key derivation function.
fn block_of(salt: &[u8], block: u32) -> Vec<u8> {
    let mut input = salt.to_vec();
    input.extend_from_slice(&block.to_be_bytes());
    input
}

/// The HMAC of SHA-512, as RFC 2104 says.
fn hmac_sha512(key: &[u8], message: &[u8]) -> [u8; 64] {
    const BLOCK: usize = 128;
    let mut padded = [0u8; BLOCK];
    if key.len() > BLOCK {
        let digest = Sha512::digest(key);
        padded[..64].copy_from_slice(&digest[..]);
    } else {
        padded[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36u8; BLOCK];
    let mut outer_pad = [0x5cu8; BLOCK];
    for index in 0..BLOCK {
        inner_pad[index] ^= padded[index];
        outer_pad[index] ^= padded[index];
    }
    let mut inner = Sha512::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha512::new();
    outer.update(outer_pad);
    outer.update(inner);
    let digest = outer.finalize();
    let mut out = [0u8; 64];
    out.copy_from_slice(&digest[..]);
    out
}

/// The old SHA-1 hash the core used before the salted SHA-512.
fn sha1(data: &[u8]) -> [u8; 20] {
    // The core only kept SHA-1 hashes around for databases created before
    // 0.13; such a database is upgraded on the first login.  We only need to
    // be able to verify such a hash, so a small SHA-1 implementation is
    // enough.
    let mut state: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut message = data.to_vec();
    let bit_length = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());

    for chunk in message.chunks_exact(64) {
        let mut words = [0u32; 80];
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (index, word) in words.iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9BA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
    }

    let mut digest = [0u8; 20];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// A hex string of the salt.
fn random_salt() -> Result<String, StorageError> {
    let mut bytes = [0u8; SALT_BYTES];
    let mut file = std::fs::File::open("/dev/urandom")
        .map_err(|error| StorageError::Random(error.to_string()))?;
    use std::io::Read;
    file.read_exact(&mut bytes)
        .map_err(|error| StorageError::Random(error.to_string()))?;
    Ok(hex(&bytes))
}

/// Lower case hex, the way Qt writes it.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_hash_round_trips() {
        let (hashed, version) = hash_password("secret").unwrap();
        assert_eq!(version, HASH_VERSION_PBKDF2);
        assert!(check_password("secret", &hashed, version));
        assert!(!check_password("wrong", &hashed, version));
    }

    #[test]
    fn a_new_hash_names_the_scheme_and_its_work_factor() {
        let (hashed, _) = hash_password("secret").unwrap();
        assert!(hashed.starts_with("pbkdf2-sha512$"), "{hashed}");
        assert!(
            hashed.contains(&format!("${PBKDF2_ITERATIONS}$")),
            "{hashed}"
        );
    }

    #[test]
    fn the_salt_is_not_the_same_twice() {
        let (first, _) = hash_password("secret").unwrap();
        let (second, _) = hash_password("secret").unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn the_old_sha1_hash_is_still_checkable() {
        let hashed = hex(&sha1(b"secret"));
        assert!(check_password("secret", &hashed, HASH_VERSION_SHA1));
        assert!(!check_password("wrong", &hashed, HASH_VERSION_SHA1));
    }

    #[test]
    fn the_old_sha512_hash_is_still_checkable() {
        let hashed = sha512_with_salt("secret", "abcd");
        assert!(check_password("secret", &hashed, HASH_VERSION_SHA2_512));
        assert!(!check_password("wrong", &hashed, HASH_VERSION_SHA2_512));
    }

    #[test]
    fn a_hash_with_a_broken_work_factor_is_refused() {
        // A database that asks for a thousand million rounds must not make
        // the core spend its time on it.
        let hashed = "pbkdf2-sha512$1000000000$abcd$00";
        assert!(!check_password("secret", hashed, HASH_VERSION_PBKDF2));
    }

    #[test]
    fn the_hmac_is_the_one_of_rfc_4231() {
        // RFC 4231, test case 1.
        let key = [0x0bu8; 20];
        let digest = hmac_sha512(&key, b"Hi There");
        assert_eq!(
            hex(&digest),
            "87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cde\
             daa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854"
                .replace(' ', "")
        );
    }

    #[test]
    fn the_key_derivation_function_matches_openssl() {
        // The value is the one `openssl kdf -keylen 64 -kdfopt digest:SHA512
        // -kdfopt pass:password -kdfopt salt:salt -kdfopt iter:1 PBKDF2` writes.
        assert_eq!(
            hex(&pbkdf2(b"password", b"salt", 1, 64)),
            "867f70cf1ade02cff3752599a3a53dc4af34c7a669815ae5d513554e1c8cf252\
             c02d470a285a0501bad999bfe943c08f050235d7d68b1da55e63f73b60a57fce"
                .replace(' ', "")
        );
        assert_eq!(
            hex(&pbkdf2(b"password", b"salt", 4096, 64)),
            "d197b1b33db0143e018b12f3d1d1479e6cdebdcc97c5c0f87f6902e072f457b5\
             143f30602641b3d55cd335988cb36b84376060ecd532e039b742a239434af2d5"
                .replace(' ', "")
        );
    }
}
