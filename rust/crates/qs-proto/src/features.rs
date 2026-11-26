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
The Quassel feature set.

Features are negotiated by name; the first sixteen of them also have a bit in
the legacy feature mask, which the older protocol versions use.
*/

use std::collections::BTreeSet;

/// Every feature the core knows, with the bit it has in the legacy mask.
///
/// The names are part of the protocol: they are written as strings and must
/// not be renamed.  The bit is the one the feature has in the legacy feature
/// mask, or `None` for the features that only exist in the extended (name
/// based) negotiation.
///
/// A feature and its bit live in one table on purpose.  Two separate lists
/// could drift apart: adding a feature to one of them and forgetting the
/// other still compiles, and the feature then gets the bit of another one.
pub const FEATURES: [(&str, Option<u32>); 23] = [
    ("SynchronizedMarkerLine", Some(0x0001)),
    ("SaslAuthentication", Some(0x0002)),
    ("SaslExternal", Some(0x0004)),
    ("HideInactiveNetworks", Some(0x0008)),
    ("PasswordChange", Some(0x0010)),
    ("CapNegotiation", Some(0x0020)),
    ("VerifyServerSSL", Some(0x0040)),
    ("CustomRateLimits", Some(0x0080)),
    // 0x0100 was DccFileTransfer, which is never in use.
    ("AwayFormatTimestamp", Some(0x0200)),
    ("Authenticators", Some(0x0400)),
    ("BufferActivitySync", Some(0x0800)),
    ("CoreSideHighlights", Some(0x1000)),
    ("SenderPrefixes", Some(0x2000)),
    ("RemoteDisconnect", Some(0x4000)),
    ("ExtendedFeatures", Some(0x8000)),
    ("LongTime", None),
    ("RichMessages", None),
    ("BacklogFilterType", None),
    ("EcdsaCertfpKeys", None),
    ("LongMessageId", None),
    ("SyncedCoreInfo", None),
    ("LoadBacklogForwards", None),
    ("SkipIrcCaps", None),
];

/// A set of negotiated features.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Features {
    names: BTreeSet<String>,
}

impl Features {
    /// No features at all.
    pub fn none() -> Self {
        Self::default()
    }

    /// Every feature the core knows.
    ///
    /// This is what a default constructed `Quassel::Features` holds, and it
    /// is what the C++ client sends.
    pub fn all() -> Self {
        Self {
            names: FEATURES
                .iter()
                .map(|(name, _)| (*name).to_owned())
                .collect(),
        }
    }

    /// Builds a feature set from a list of names.
    ///
    /// Unknown names are kept, so that a client can talk to a newer core; the
    /// core logs them and carries on.
    pub fn from_names<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            names: names.into_iter().map(Into::into).collect(),
        }
    }

    /// The feature names, sorted.
    pub fn to_names(&self) -> Vec<String> {
        self.names.iter().cloned().collect()
    }

    /// The legacy bit mask of this feature set.
    pub fn to_legacy(&self) -> u32 {
        let mut mask = 0;
        for (name, bit) in FEATURES {
            if let Some(bit) = bit {
                if self.names.contains(name) {
                    mask |= bit;
                }
            }
        }
        mask
    }

    /// Builds a feature set from a legacy bit mask.
    pub fn from_legacy(mask: u32) -> Self {
        let mut names = BTreeSet::new();
        for (name, bit) in FEATURES {
            if let Some(bit) = bit {
                if mask & bit != 0 {
                    names.insert(name.to_owned());
                }
            }
        }
        Self { names }
    }

    /// Whether a feature is part of the set.
    pub fn has(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    /// Adds a feature.
    pub fn insert(&mut self, name: impl Into<String>) {
        self.names.insert(name.into());
    }

    /// The features of the set that the core does not know.
    pub fn unknown(&self) -> Vec<String> {
        self.names
            .iter()
            .filter(|name| !FEATURES.iter().any(|(known, _)| *known == name.as_str()))
            .cloned()
            .collect()
    }

    /// The number of features in the set.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bit of a feature, or `None` when the feature has none.
    fn bit_of(name: &str) -> Option<u32> {
        FEATURES
            .iter()
            .find(|(known, _)| *known == name)
            .and_then(|(_, bit)| *bit)
    }

    #[test]
    fn every_feature_has_the_bit_the_cpp_core_gives_it() {
        assert_eq!(bit_of("SynchronizedMarkerLine"), Some(0x0001));
        assert_eq!(bit_of("SaslAuthentication"), Some(0x0002));
        assert_eq!(bit_of("SaslExternal"), Some(0x0004));
        assert_eq!(bit_of("HideInactiveNetworks"), Some(0x0008));
        assert_eq!(bit_of("PasswordChange"), Some(0x0010));
        assert_eq!(bit_of("CapNegotiation"), Some(0x0020));
        assert_eq!(bit_of("VerifyServerSSL"), Some(0x0040));
        assert_eq!(bit_of("CustomRateLimits"), Some(0x0080));
        assert_eq!(bit_of("AwayFormatTimestamp"), Some(0x0200));
        assert_eq!(bit_of("Authenticators"), Some(0x0400));
        assert_eq!(bit_of("BufferActivitySync"), Some(0x0800));
        assert_eq!(bit_of("CoreSideHighlights"), Some(0x1000));
        assert_eq!(bit_of("SenderPrefixes"), Some(0x2000));
        assert_eq!(bit_of("RemoteDisconnect"), Some(0x4000));
        assert_eq!(bit_of("ExtendedFeatures"), Some(0x8000));
    }

    #[test]
    fn the_features_without_a_legacy_bit_have_none() {
        for name in [
            "LongTime",
            "RichMessages",
            "BacklogFilterType",
            "EcdsaCertfpKeys",
            "LongMessageId",
            "SyncedCoreInfo",
            "LoadBacklogForwards",
            "SkipIrcCaps",
        ] {
            assert_eq!(bit_of(name), None, "{name}");
        }
    }

    #[test]
    fn the_reserved_bit_is_not_given_to_any_feature() {
        // 0x0100 was DccFileTransfer; no feature may take it.
        assert!(FEATURES
            .iter()
            .all(|(_, bit)| bit.is_none_or(|bit| bit & 0x0100 == 0)));
    }

    #[test]
    fn no_two_features_share_a_bit() {
        let mut bits: Vec<u32> = FEATURES.iter().filter_map(|(_, bit)| *bit).collect();
        bits.sort_unstable();
        let count = bits.len();
        bits.dedup();
        assert_eq!(bits.len(), count, "two features share a legacy bit");
    }

    #[test]
    fn no_two_features_share_a_name() {
        let mut names: Vec<&str> = FEATURES.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two features share a name");
    }
}
