// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `Header Files/EngineStrings.h` — resource string IDs for the
//! private strings used by the engine.

pub const IDS_ERRORS_FIRST: i32 = 99;

// This is the list of error strings corresponding to SCERR_DIVIDEZERO..

pub const IDS_DIVBYZERO: i32 = IDS_ERRORS_FIRST;
pub const IDS_DOMAIN: i32 = IDS_ERRORS_FIRST + 1;
pub const IDS_UNDEFINED: i32 = IDS_ERRORS_FIRST + 2;
pub const IDS_POS_INFINITY: i32 = IDS_ERRORS_FIRST + 3;
pub const IDS_NEG_INFINITY: i32 = IDS_ERRORS_FIRST + 4;
pub const IDS_NOMEM: i32 = IDS_ERRORS_FIRST + 6;
pub const IDS_TOOMANY: i32 = IDS_ERRORS_FIRST + 7;
pub const IDS_OVERFLOW: i32 = IDS_ERRORS_FIRST + 8;
pub const IDS_NORESULT: i32 = IDS_ERRORS_FIRST + 9;
pub const IDS_INSUFFICIENT_DATA: i32 = IDS_ERRORS_FIRST + 10;

pub const CSTRINGSENGMAX: i32 = IDS_INSUFFICIENT_DATA + 1;

// Arithmetic expression evaluator error strings
pub const IDS_ERR_UNK_CH: i32 = CSTRINGSENGMAX + 1;
pub const IDS_ERR_UNK_FN: i32 = CSTRINGSENGMAX + 2;
pub const IDS_ERR_UNEX_NUM: i32 = CSTRINGSENGMAX + 3;
pub const IDS_ERR_UNEX_CH: i32 = CSTRINGSENGMAX + 4;
pub const IDS_ERR_UNEX_SZ: i32 = CSTRINGSENGMAX + 5;
pub const IDS_ERR_MISMATCH_CLOSE: i32 = CSTRINGSENGMAX + 6;
pub const IDS_ERR_UNEX_END: i32 = CSTRINGSENGMAX + 7;
pub const IDS_ERR_SG_INV_ERROR: i32 = CSTRINGSENGMAX + 8;
pub const IDS_ERR_INPUT_OVERFLOW: i32 = CSTRINGSENGMAX + 9;
pub const IDS_ERR_OUTPUT_OVERFLOW: i32 = CSTRINGSENGMAX + 10;

// Resource keys for CEngineStrings.resw
pub const SIDS_PLUS_MINUS: &str = "0";
pub const SIDS_CLEAR: &str = "1";
pub const SIDS_CE: &str = "2";
pub const SIDS_BACKSPACE: &str = "3";
pub const SIDS_DECIMAL_SEPARATOR: &str = "4";
pub const SIDS_EMPTY_STRING: &str = "5";
pub const SIDS_AND: &str = "6";
pub const SIDS_OR: &str = "7";
pub const SIDS_XOR: &str = "8";
pub const SIDS_LSH: &str = "9";
pub const SIDS_RSH: &str = "10";
pub const SIDS_DIVIDE: &str = "11";
pub const SIDS_MULTIPLY: &str = "12";
pub const SIDS_PLUS: &str = "13";
pub const SIDS_MINUS: &str = "14";
pub const SIDS_MOD: &str = "15";
pub const SIDS_YROOT: &str = "16";
pub const SIDS_POW_HAT: &str = "17";
pub const SIDS_INT: &str = "18";
pub const SIDS_ROL: &str = "19";
pub const SIDS_ROR: &str = "20";
pub const SIDS_NOT: &str = "21";
pub const SIDS_SIN: &str = "22";
pub const SIDS_COS: &str = "23";
pub const SIDS_TAN: &str = "24";
pub const SIDS_SINH: &str = "25";
pub const SIDS_COSH: &str = "26";
pub const SIDS_TANH: &str = "27";
pub const SIDS_LN: &str = "28";
pub const SIDS_LOG: &str = "29";
pub const SIDS_SQRT: &str = "30";
pub const SIDS_XPOW2: &str = "31";
pub const SIDS_XPOW3: &str = "32";
pub const SIDS_NFACTORIAL: &str = "33";
pub const SIDS_RECIPROCAL: &str = "34";
pub const SIDS_DMS: &str = "35";
pub const SIDS_POWTEN: &str = "37";
pub const SIDS_PERCENT: &str = "38";
pub const SIDS_SCIENTIFIC_NOTATION: &str = "39";
pub const SIDS_PI: &str = "40";
pub const SIDS_EQUAL: &str = "41";
pub const SIDS_MC: &str = "42";
pub const SIDS_MR: &str = "43";
pub const SIDS_MS: &str = "44";
pub const SIDS_MPLUS: &str = "45";
pub const SIDS_MMINUS: &str = "46";
pub const SIDS_EXP: &str = "47";
pub const SIDS_OPEN_PAREN: &str = "48";
pub const SIDS_CLOSE_PAREN: &str = "49";
pub const SIDS_0: &str = "50";
pub const SIDS_1: &str = "51";
pub const SIDS_2: &str = "52";
pub const SIDS_3: &str = "53";
pub const SIDS_4: &str = "54";
pub const SIDS_5: &str = "55";
pub const SIDS_6: &str = "56";
pub const SIDS_7: &str = "57";
pub const SIDS_8: &str = "58";
pub const SIDS_9: &str = "59";
pub const SIDS_A: &str = "60";
pub const SIDS_B: &str = "61";
pub const SIDS_C: &str = "62";
pub const SIDS_D: &str = "63";
pub const SIDS_E: &str = "64";
pub const SIDS_F: &str = "65";
pub const SIDS_FRAC: &str = "66";
pub const SIDS_SIND: &str = "67";
pub const SIDS_COSD: &str = "68";
pub const SIDS_TAND: &str = "69";
pub const SIDS_ASIND: &str = "70";
pub const SIDS_ACOSD: &str = "71";
pub const SIDS_ATAND: &str = "72";
pub const SIDS_SINR: &str = "73";
pub const SIDS_COSR: &str = "74";
pub const SIDS_TANR: &str = "75";
pub const SIDS_ASINR: &str = "76";
pub const SIDS_ACOSR: &str = "77";
pub const SIDS_ATANR: &str = "78";
pub const SIDS_SING: &str = "79";
pub const SIDS_COSG: &str = "80";
pub const SIDS_TANG: &str = "81";
pub const SIDS_ASING: &str = "82";
pub const SIDS_ACOSG: &str = "83";
pub const SIDS_ATANG: &str = "84";
pub const SIDS_ASINH: &str = "85";
pub const SIDS_ACOSH: &str = "86";
pub const SIDS_ATANH: &str = "87";
pub const SIDS_POWE: &str = "88";
pub const SIDS_POWTEN2: &str = "89";
pub const SIDS_SQRT2: &str = "90";
pub const SIDS_SQR: &str = "91";
pub const SIDS_CUBE: &str = "92";
pub const SIDS_CUBERT: &str = "93";
pub const SIDS_FACT: &str = "94";
pub const SIDS_RECIPROC: &str = "95";
pub const SIDS_DEGREES: &str = "96";
pub const SIDS_NEGATE: &str = "97";
pub const SIDS_RSH2: &str = "98";
pub const SIDS_DIVIDEBYZERO: &str = "99";
pub const SIDS_DOMAIN: &str = "100";
pub const SIDS_UNDEFINED: &str = "101";
pub const SIDS_POS_INFINITY: &str = "102";
pub const SIDS_NEG_INFINITY: &str = "103";
pub const SIDS_ABORTED: &str = "104";
pub const SIDS_NOMEM: &str = "105";
pub const SIDS_TOOMANY: &str = "106";
pub const SIDS_OVERFLOW: &str = "107";
pub const SIDS_NORESULT: &str = "108";
pub const SIDS_INSUFFICIENT_DATA: &str = "109";
// 110 is skipped by CSTRINGSENGMAX
pub const SIDS_ERR_UNK_CH: &str = "111";
pub const SIDS_ERR_UNK_FN: &str = "112";
pub const SIDS_ERR_UNEX_NUM: &str = "113";
pub const SIDS_ERR_UNEX_CH: &str = "114";
pub const SIDS_ERR_UNEX_SZ: &str = "115";
pub const SIDS_ERR_MISMATCH_CLOSE: &str = "116";
pub const SIDS_ERR_UNEX_END: &str = "117";
pub const SIDS_ERR_SG_INV_ERROR: &str = "118";
pub const SIDS_ERR_INPUT_OVERFLOW: &str = "119";
pub const SIDS_ERR_OUTPUT_OVERFLOW: &str = "120";
pub const SIDS_SECD: &str = "SecDeg";
pub const SIDS_SECR: &str = "SecRad";
pub const SIDS_SECG: &str = "SecGrad";
pub const SIDS_ASECD: &str = "InverseSecDeg";
pub const SIDS_ASECR: &str = "InverseSecRad";
pub const SIDS_ASECG: &str = "InverseSecGrad";
pub const SIDS_CSCD: &str = "CscDeg";
pub const SIDS_CSCR: &str = "CscRad";
pub const SIDS_CSCG: &str = "CscGrad";
pub const SIDS_ACSCD: &str = "InverseCscDeg";
pub const SIDS_ACSCR: &str = "InverseCscRad";
pub const SIDS_ACSCG: &str = "InverseCscGrad";
pub const SIDS_COTD: &str = "CotDeg";
pub const SIDS_COTR: &str = "CotRad";
pub const SIDS_COTG: &str = "CotGrad";
pub const SIDS_ACOTD: &str = "InverseCotDeg";
pub const SIDS_ACOTR: &str = "InverseCotRad";
pub const SIDS_ACOTG: &str = "InverseCotGrad";
pub const SIDS_SECH: &str = "Sech";
pub const SIDS_ASECH: &str = "InverseSech";
pub const SIDS_CSCH: &str = "Csch";
pub const SIDS_ACSCH: &str = "InverseCsch";
pub const SIDS_COTH: &str = "Coth";
pub const SIDS_ACOTH: &str = "InverseCoth";
pub const SIDS_TWOPOWX: &str = "TwoPowX";
pub const SIDS_LOGBASEY: &str = "LogBaseY";
pub const SIDS_ABS: &str = "Abs";
pub const SIDS_FLOOR: &str = "Floor";
pub const SIDS_CEIL: &str = "Ceil";
pub const SIDS_NAND: &str = "Nand";
pub const SIDS_NOR: &str = "Nor";
pub const SIDS_CUBEROOT: &str = "CubeRoot";
pub const SIDS_PROGRAMMER_MOD: &str = "ProgrammerMod";

/// Include the resource key ID from above into this vector to load it into
/// memory for the engine to use. (Kept bit-for-bit identical to the C++
/// table, including its quirks: entry 1 is `SIDS_C` and entry 97 is
/// `SIDS_RSH` rather than `SIDS_CLEAR` / `SIDS_RSH2`.)
pub const G_SIDS: [&str; 152] = [
    SIDS_PLUS_MINUS,
    SIDS_C,
    SIDS_CE,
    SIDS_BACKSPACE,
    SIDS_DECIMAL_SEPARATOR,
    SIDS_EMPTY_STRING,
    SIDS_AND,
    SIDS_OR,
    SIDS_XOR,
    SIDS_LSH,
    SIDS_RSH,
    SIDS_DIVIDE,
    SIDS_MULTIPLY,
    SIDS_PLUS,
    SIDS_MINUS,
    SIDS_MOD,
    SIDS_YROOT,
    SIDS_POW_HAT,
    SIDS_INT,
    SIDS_ROL,
    SIDS_ROR,
    SIDS_NOT,
    SIDS_SIN,
    SIDS_COS,
    SIDS_TAN,
    SIDS_SINH,
    SIDS_COSH,
    SIDS_TANH,
    SIDS_LN,
    SIDS_LOG,
    SIDS_SQRT,
    SIDS_XPOW2,
    SIDS_XPOW3,
    SIDS_NFACTORIAL,
    SIDS_RECIPROCAL,
    SIDS_DMS,
    SIDS_POWTEN,
    SIDS_PERCENT,
    SIDS_SCIENTIFIC_NOTATION,
    SIDS_PI,
    SIDS_EQUAL,
    SIDS_MC,
    SIDS_MR,
    SIDS_MS,
    SIDS_MPLUS,
    SIDS_MMINUS,
    SIDS_EXP,
    SIDS_OPEN_PAREN,
    SIDS_CLOSE_PAREN,
    SIDS_0,
    SIDS_1,
    SIDS_2,
    SIDS_3,
    SIDS_4,
    SIDS_5,
    SIDS_6,
    SIDS_7,
    SIDS_8,
    SIDS_9,
    SIDS_A,
    SIDS_B,
    SIDS_C,
    SIDS_D,
    SIDS_E,
    SIDS_F,
    SIDS_FRAC,
    SIDS_SIND,
    SIDS_COSD,
    SIDS_TAND,
    SIDS_ASIND,
    SIDS_ACOSD,
    SIDS_ATAND,
    SIDS_SINR,
    SIDS_COSR,
    SIDS_TANR,
    SIDS_ASINR,
    SIDS_ACOSR,
    SIDS_ATANR,
    SIDS_SING,
    SIDS_COSG,
    SIDS_TANG,
    SIDS_ASING,
    SIDS_ACOSG,
    SIDS_ATANG,
    SIDS_ASINH,
    SIDS_ACOSH,
    SIDS_ATANH,
    SIDS_POWE,
    SIDS_POWTEN2,
    SIDS_SQRT2,
    SIDS_SQR,
    SIDS_CUBE,
    SIDS_CUBERT,
    SIDS_FACT,
    SIDS_RECIPROC,
    SIDS_DEGREES,
    SIDS_NEGATE,
    SIDS_RSH,
    SIDS_DIVIDEBYZERO,
    SIDS_DOMAIN,
    SIDS_UNDEFINED,
    SIDS_POS_INFINITY,
    SIDS_NEG_INFINITY,
    SIDS_ABORTED,
    SIDS_NOMEM,
    SIDS_TOOMANY,
    SIDS_OVERFLOW,
    SIDS_NORESULT,
    SIDS_INSUFFICIENT_DATA,
    SIDS_ERR_UNK_CH,
    SIDS_ERR_UNK_FN,
    SIDS_ERR_UNEX_NUM,
    SIDS_ERR_UNEX_CH,
    SIDS_ERR_UNEX_SZ,
    SIDS_ERR_MISMATCH_CLOSE,
    SIDS_ERR_UNEX_END,
    SIDS_ERR_SG_INV_ERROR,
    SIDS_ERR_INPUT_OVERFLOW,
    SIDS_ERR_OUTPUT_OVERFLOW,
    SIDS_SECD,
    SIDS_SECG,
    SIDS_SECR,
    SIDS_ASECD,
    SIDS_ASECR,
    SIDS_ASECG,
    SIDS_CSCD,
    SIDS_CSCR,
    SIDS_CSCG,
    SIDS_ACSCD,
    SIDS_ACSCR,
    SIDS_ACSCG,
    SIDS_COTD,
    SIDS_COTR,
    SIDS_COTG,
    SIDS_ACOTD,
    SIDS_ACOTR,
    SIDS_ACOTG,
    SIDS_SECH,
    SIDS_ASECH,
    SIDS_CSCH,
    SIDS_ACSCH,
    SIDS_COTH,
    SIDS_ACOTH,
    SIDS_TWOPOWX,
    SIDS_LOGBASEY,
    SIDS_ABS,
    SIDS_FLOOR,
    SIDS_CEIL,
    SIDS_NAND,
    SIDS_NOR,
    SIDS_CUBEROOT,
    SIDS_PROGRAMMER_MOD,
];
