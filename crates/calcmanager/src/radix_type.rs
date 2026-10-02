// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `Header Files/RadixType.h`.

/// This is expected to be in same order as IDM_HEX, IDM_DEC, IDM_OCT, IDM_BIN
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RadixType {
    Hex = 0,
    Decimal = 1,
    Octal = 2,
    Binary = 3,
}

impl RadixType {
    /// `(RadixType)i` for `i` in `0..=3`.
    pub fn from_index(i: i32) -> Option<RadixType> {
        match i {
            0 => Some(RadixType::Hex),
            1 => Some(RadixType::Decimal),
            2 => Some(RadixType::Octal),
            3 => Some(RadixType::Binary),
            _ => None,
        }
    }
}
