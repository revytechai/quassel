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
Error type for the QDataStream codec.
*/

use std::fmt;

/// Everything that can go wrong while decoding a Quassel DataStream message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The buffer ended in the middle of a value.
    UnexpectedEof {
        /// What the decoder was trying to read.
        wanted: &'static str,
    },
    /// A `QVariant` carried a type id the core does not know.
    UnknownVariantType(u32),
    /// A user type carried a name the core does not know.
    ///
    /// The C++ core rejects these with "Type name is not valid"; the name is
    /// the wire name (for example `Network::Server`, not `Network_Server`).
    InvalidUserType(String),
    /// A length prefix was larger than the codec is willing to allocate.
    TooLarge {
        /// The length that was read off the wire.
        length: u64,
        /// The maximum the codec accepts.
        limit: u64,
    },
    /// A value was nested deeper than the codec is willing to recurse.
    ///
    /// The decoder walks into nested values, so a value that nests deeply
    /// enough would exhaust the stack.  The codec stops at a fixed depth
    /// instead.
    TooDeep {
        /// The depth the value was nested at.
        depth: u32,
        /// The maximum depth the codec accepts.
        limit: u32,
    },
    /// A string's UTF-16 payload could not be turned into text.
    ///
    /// A payload with an odd byte count, or one that carries an unpaired
    /// surrogate, is not a string the core could have written.
    InvalidUtf16,
    /// A map key was null.
    ///
    /// A `QVariantMap` key is a plain string; a null key has no name to
    /// store, so the map cannot be built.
    NullMapKey,
    /// Trailing bytes after a complete value.
    TrailingBytes(usize),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { wanted } => {
                write!(f, "unexpected end of input while reading {wanted}")
            }
            Self::UnknownVariantType(t) => write!(f, "unknown variant type {t}"),
            Self::InvalidUserType(name) => write!(f, "type name is not valid: {name}"),
            Self::TooLarge { length, limit } => {
                write!(f, "length {length} exceeds the limit of {limit}")
            }
            Self::TooDeep { depth, limit } => {
                write!(f, "value is nested {depth} deep, the limit is {limit}")
            }
            Self::InvalidUtf16 => write!(f, "string is not valid UTF-16"),
            Self::NullMapKey => write!(f, "map key is null"),
            Self::TrailingBytes(n) => write!(f, "{n} trailing bytes after the value"),
        }
    }
}

impl std::error::Error for DecodeError {}
