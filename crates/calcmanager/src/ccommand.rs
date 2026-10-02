// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `Header Files/CCommand.h` — resource IDs for the engine commands.
//!
//! These are the valid ids which can be passed to `CalcEngine::process_command`.

/// `OpCode` (C++: `uintptr_t`). Every valid op code is a small positive
/// number, so `i32` is used throughout the port.
pub type OpCode = i32;

pub const IDM_HEX: i32 = 313;
pub const IDM_DEC: i32 = 314;
pub const IDM_OCT: i32 = 315;
pub const IDM_BIN: i32 = 316;
pub const IDM_QWORD: i32 = 317;
pub const IDM_DWORD: i32 = 318;
pub const IDM_WORD: i32 = 319;
pub const IDM_BYTE: i32 = 320;
pub const IDM_DEG: i32 = 321;
pub const IDM_RAD: i32 = 322;
pub const IDM_GRAD: i32 = 323;
pub const IDM_DEGREES: i32 = 324;

pub const IDC_HEX: i32 = IDM_HEX;
pub const IDC_DEC: i32 = IDM_DEC;
pub const IDC_OCT: i32 = IDM_OCT;
pub const IDC_BIN: i32 = IDM_BIN;

pub const IDC_DEG: i32 = IDM_DEG;
pub const IDC_RAD: i32 = IDM_RAD;
pub const IDC_GRAD: i32 = IDM_GRAD;
pub const IDC_DEGREES: i32 = IDM_DEGREES;

pub const IDC_QWORD: i32 = IDM_QWORD;
pub const IDC_DWORD: i32 = IDM_DWORD;
pub const IDC_WORD: i32 = IDM_WORD;
pub const IDC_BYTE: i32 = IDM_BYTE;

// Key IDs:
// These id's must be consecutive from IDC_FIRSTCONTROL to IDC_LASTCONTROL.
// The actual values don't matter but the order and sequence are very important.
// Also, the order of the controls must match the order of the control names
// in the string table.
pub const IDC_FIRSTCONTROL: i32 = IDC_SIGN;
pub const IDC_SIGN: i32 = 80;
pub const IDC_CLEAR: i32 = 81;
pub const IDC_CENTR: i32 = 82;
pub const IDC_BACK: i32 = 83;

pub const IDC_PNT: i32 = 84;

// Hole  85

pub const IDC_AND: i32 = 86; // Binary operators must be between IDC_AND and IDC_PWR
pub const IDC_OR: i32 = 87;
pub const IDC_XOR: i32 = 88;
pub const IDC_LSHF: i32 = 89;
pub const IDC_RSHF: i32 = 90;
pub const IDC_DIV: i32 = 91;
pub const IDC_MUL: i32 = 92;
pub const IDC_ADD: i32 = 93;
pub const IDC_SUB: i32 = 94;
pub const IDC_MOD: i32 = 95;
pub const IDC_ROOT: i32 = 96;
pub const IDC_PWR: i32 = 97;

pub const IDC_UNARYFIRST: i32 = IDC_CHOP;
pub const IDC_CHOP: i32 = 98; // Unary operators must be between IDC_CHOP and IDC_EQU
pub const IDC_ROL: i32 = 99;
pub const IDC_ROR: i32 = 100;
pub const IDC_COM: i32 = 101;

pub const IDC_SIN: i32 = 102;
pub const IDC_COS: i32 = 103;
pub const IDC_TAN: i32 = 104;

pub const IDC_SINH: i32 = 105;
pub const IDC_COSH: i32 = 106;
pub const IDC_TANH: i32 = 107;

pub const IDC_LN: i32 = 108;
pub const IDC_LOG: i32 = 109;
pub const IDC_SQRT: i32 = 110;
pub const IDC_SQR: i32 = 111;
pub const IDC_CUB: i32 = 112;
pub const IDC_FAC: i32 = 113;
pub const IDC_REC: i32 = 114;
pub const IDC_DMS: i32 = 115;
pub const IDC_CUBEROOT: i32 = 116; // x ^ 1/3
pub const IDC_POW10: i32 = 117; // 10 ^ x
pub const IDC_PERCENT: i32 = 118;
pub const IDC_UNARYLAST: i32 = IDC_PERCENT;

pub const IDC_FE: i32 = 119;
pub const IDC_PI: i32 = 120;
pub const IDC_EQU: i32 = 121;

pub const IDC_MCLEAR: i32 = 122;
pub const IDC_RECALL: i32 = 123;
pub const IDC_STORE: i32 = 124;
pub const IDC_MPLUS: i32 = 125;
pub const IDC_MMINUS: i32 = 126;

pub const IDC_EXP: i32 = 127;

pub const IDC_OPENP: i32 = 128;
pub const IDC_CLOSEP: i32 = 129;

pub const IDC_0: i32 = 130; // The controls for 0 through F must be consecutive and in order
pub const IDC_1: i32 = 131;
pub const IDC_2: i32 = 132;
pub const IDC_3: i32 = 133;
pub const IDC_4: i32 = 134;
pub const IDC_5: i32 = 135;
pub const IDC_6: i32 = 136;
pub const IDC_7: i32 = 137;
pub const IDC_8: i32 = 138;
pub const IDC_9: i32 = 139;
pub const IDC_A: i32 = 140;
pub const IDC_B: i32 = 141;
pub const IDC_C: i32 = 142;
pub const IDC_D: i32 = 143;
pub const IDC_E: i32 = 144;
pub const IDC_F: i32 = 145; // this is last control ID which must match the string table
pub const IDC_INV: i32 = 146;
pub const IDC_SET_RESULT: i32 = 147;

pub const IDC_STRING_MAPPED_VALUES: i32 = 400;
pub const IDC_UNARYEXTENDEDFIRST: i32 = IDC_STRING_MAPPED_VALUES;
pub const IDC_SEC: i32 = 400; // Secant
// 401 reserved for inverse
pub const IDC_CSC: i32 = 402; // Cosecant
// 403 reserved for inverse
pub const IDC_COT: i32 = 404; // Cotangent
// 405 reserved for inverse

pub const IDC_SECH: i32 = 406; // Hyperbolic Secant
// 407 reserved for inverse
pub const IDC_CSCH: i32 = 408; // Hyperbolic Cosecant
// 409 reserved for inverse
pub const IDC_COTH: i32 = 410; // Hyperbolic Cotangent
// 411 reserved for inverse

pub const IDC_POW2: i32 = 412; // 2 ^ x
pub const IDC_ABS: i32 = 413; // Absolute Value
pub const IDC_FLOOR: i32 = 414; // Floor
pub const IDC_CEIL: i32 = 415; // Ceiling

pub const IDC_ROLC: i32 = 416; // Rotate Left Circular
pub const IDC_RORC: i32 = 417; // Rotate Right Circular

pub const IDC_UNARYEXTENDEDLAST: i32 = IDC_RORC;

pub const IDC_LASTCONTROL: i32 = IDC_CEIL;

pub const IDC_BINARYEXTENDEDFIRST: i32 = 500;
pub const IDC_LOGBASEY: i32 = 500; // logy(x)
pub const IDC_NAND: i32 = 501; // Nand
pub const IDC_NOR: i32 = 502; // Nor

pub const IDC_RSHFL: i32 = 505; // Right Shift Logical
pub const IDC_BINARYEXTENDEDLAST: i32 = IDC_RSHFL;

pub const IDC_RAND: i32 = 600; // Random
pub const IDC_EULER: i32 = 601; // e Constant

pub const IDC_BINEDITSTART: i32 = 700;
pub const IDC_BINPOS0: i32 = 700;
pub const IDC_BINPOS63: i32 = 763;
pub const IDC_BINEDITEND: i32 = 763;

// The strings in the following range IDS_ENGINESTR_FIRST ... IDS_ENGINESTR_MAX are strings allocated in the
// resource for the purpose internal to Engine and cant be used by the clients
pub const IDS_ENGINESTR_FIRST: i32 = 0;
pub const IDS_ENGINESTR_MAX: i32 = 200;
