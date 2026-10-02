// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CEngine/CalcUtils.cpp` / `Header Files/CalcUtils.h`.

use crate::ccommand::*;

pub fn is_op_in_range(op: OpCode, x: i32, y: i32) -> bool {
    (op >= x) && (op <= y)
}

pub fn is_bin_op_code(op_code: OpCode) -> bool {
    is_op_in_range(op_code, IDC_AND, IDC_PWR)
        || is_op_in_range(op_code, IDC_BINARYEXTENDEDFIRST, IDC_BINARYEXTENDEDLAST)
}

/// WARNING: IDC_SIGN is a special unary op but still this doesn't catch this. Caller has to be aware
/// of it and catch it themselves or not needing this
pub fn is_unary_op_code(op_code: OpCode) -> bool {
    is_op_in_range(op_code, IDC_UNARYFIRST, IDC_UNARYLAST)
        || is_op_in_range(op_code, IDC_UNARYEXTENDEDFIRST, IDC_UNARYEXTENDEDLAST)
}

pub fn is_digit_op_code(op_code: OpCode) -> bool {
    is_op_in_range(op_code, IDC_0, IDC_F)
}

/// Some commands are not affecting the state machine state of the calc flow. But these are more of
/// some gui mode kind of settings (eg Inv button, or Deg,Rad , Back etc.). This list is getting bigger & bigger
/// so we abstract this as a separate routine. Note: There is another side to this. Some commands are not
/// gui mode setting to begin with, but once it is discovered it is invalid and we want to behave as though it
/// was never inout, we need to revert the state changes made as a result of this test
pub fn is_gui_setting_op_code(op_code: OpCode) -> bool {
    if is_op_in_range(op_code, IDM_HEX, IDM_BIN)
        || is_op_in_range(op_code, IDM_QWORD, IDM_BYTE)
        || is_op_in_range(op_code, IDM_DEG, IDM_GRAD)
    {
        return true;
    }

    matches!(
        op_code,
        IDC_INV | IDC_FE | IDC_MCLEAR | IDC_BACK | IDC_EXP | IDC_STORE | IDC_MPLUS | IDC_MMINUS
    )
}
