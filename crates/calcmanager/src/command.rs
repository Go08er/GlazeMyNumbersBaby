// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `Command.h` (the `CalculationManager` part) plus the small enums
//! declared in `CalculatorManager.h`.
//!
//! `CalculationManager::Command` is an `enum class` whose values are cast
//! freely to and from `int` (and which contains duplicate values such as
//! `CommandNot == CommandCOM`), so it is modelled as a transparent newtype
//! over `i32` with associated constants that keep the exact C++ names and
//! numeric values. ExpressionCommand serialization depends on these numbers.

/// `CalculationManager::CommandType`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandType {
    UnaryCommand,
    BinaryCommand,
    OperandCommand,
    Parentheses,
}

/// `CalculationManager::Command` — any `i32` is representable, exactly like
/// the C++ `enum class` that is `static_cast` from arbitrary ints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Command(pub i32);

impl From<i32> for Command {
    fn from(v: i32) -> Self {
        Command(v)
    }
}

impl From<Command> for i32 {
    fn from(c: Command) -> Self {
        c.0
    }
}

#[allow(non_upper_case_globals)]
impl Command {
    // Commands for programmer calculators are omitted.
    pub const CommandDEG: Command = Command(321);
    pub const CommandRAD: Command = Command(322);
    pub const CommandGRAD: Command = Command(323);
    pub const CommandDegrees: Command = Command(324);
    pub const CommandHYP: Command = Command(325);

    pub const CommandNULL: Command = Command(0);

    pub const CommandSIGN: Command = Command(80);
    pub const CommandCLEAR: Command = Command(81);
    pub const CommandCENTR: Command = Command(82);
    pub const CommandBACK: Command = Command(83);

    pub const CommandPNT: Command = Command(84);

    // Hole  85
    // Unused commands defined in Command.h is omitted.
    pub const CommandXor: Command = Command(88);
    pub const CommandLSHF: Command = Command(89);
    pub const CommandRSHF: Command = Command(90);
    pub const CommandDIV: Command = Command(91);
    pub const CommandMUL: Command = Command(92);
    pub const CommandADD: Command = Command(93);
    pub const CommandSUB: Command = Command(94);
    pub const CommandMOD: Command = Command(95);
    pub const CommandROOT: Command = Command(96);
    pub const CommandPWR: Command = Command(97);

    pub const CommandCHOP: Command = Command(98); // Unary operators must be between CommandCHOP and CommandEQU
    pub const CommandROL: Command = Command(99);
    pub const CommandROR: Command = Command(100);
    pub const CommandCOM: Command = Command(101);

    pub const CommandSIN: Command = Command(102);
    pub const CommandCOS: Command = Command(103);
    pub const CommandTAN: Command = Command(104);

    pub const CommandSINH: Command = Command(105);
    pub const CommandCOSH: Command = Command(106);
    pub const CommandTANH: Command = Command(107);

    pub const CommandLN: Command = Command(108);
    pub const CommandLOG: Command = Command(109);
    pub const CommandSQRT: Command = Command(110);
    pub const CommandSQR: Command = Command(111);
    pub const CommandCUB: Command = Command(112);
    pub const CommandFAC: Command = Command(113);
    pub const CommandREC: Command = Command(114);
    pub const CommandDMS: Command = Command(115);
    pub const CommandCUBEROOT: Command = Command(116); // x ^ 1/3
    pub const CommandPOW10: Command = Command(117); // 10 ^ x
    pub const CommandPERCENT: Command = Command(118);

    pub const CommandFE: Command = Command(119);
    pub const CommandPI: Command = Command(120);
    pub const CommandEQU: Command = Command(121);

    pub const CommandMCLEAR: Command = Command(122);
    pub const CommandRECALL: Command = Command(123);
    pub const CommandSTORE: Command = Command(124);
    pub const CommandMPLUS: Command = Command(125);
    pub const CommandMMINUS: Command = Command(126);

    pub const CommandEXP: Command = Command(127);

    pub const CommandOPENP: Command = Command(128);
    pub const CommandCLOSEP: Command = Command(129);

    pub const Command0: Command = Command(130); // The controls for 0 through F must be consecutive and in order
    pub const Command1: Command = Command(131);
    pub const Command2: Command = Command(132);
    pub const Command3: Command = Command(133);
    pub const Command4: Command = Command(134);
    pub const Command5: Command = Command(135);
    pub const Command6: Command = Command(136);
    pub const Command7: Command = Command(137);
    pub const Command8: Command = Command(138);
    pub const Command9: Command = Command(139);
    pub const CommandA: Command = Command(140);
    pub const CommandB: Command = Command(141);
    pub const CommandC: Command = Command(142);
    pub const CommandD: Command = Command(143);
    pub const CommandE: Command = Command(144);
    pub const CommandF: Command = Command(145); // this is last control ID which must match the string table
    pub const CommandINV: Command = Command(146);
    pub const CommandSET_RESULT: Command = Command(147);

    pub const CommandSEC: Command = Command(400);
    pub const CommandASEC: Command = Command(401);
    pub const CommandCSC: Command = Command(402);
    pub const CommandACSC: Command = Command(403);
    pub const CommandCOT: Command = Command(404);
    pub const CommandACOT: Command = Command(405);

    pub const CommandSECH: Command = Command(406);
    pub const CommandASECH: Command = Command(407);
    pub const CommandCSCH: Command = Command(408);
    pub const CommandACSCH: Command = Command(409);
    pub const CommandCOTH: Command = Command(410);
    pub const CommandACOTH: Command = Command(411);

    pub const CommandPOW2: Command = Command(412); // 2 ^ x
    pub const CommandAbs: Command = Command(413);
    pub const CommandFloor: Command = Command(414);
    pub const CommandCeil: Command = Command(415);
    pub const CommandROLC: Command = Command(416);
    pub const CommandRORC: Command = Command(417);
    pub const CommandLogBaseY: Command = Command(500);
    pub const CommandNand: Command = Command(501);
    pub const CommandNor: Command = Command(502);

    pub const CommandRSHFL: Command = Command(505);
    pub const CommandRand: Command = Command(600);
    pub const CommandEuler: Command = Command(601);

    pub const CommandAnd: Command = Command(86);
    pub const CommandOR: Command = Command(87);
    pub const CommandNot: Command = Command(101);

    pub const ModeBasic: Command = Command(200);
    pub const ModeScientific: Command = Command(201);

    pub const CommandASIN: Command = Command(202);
    pub const CommandACOS: Command = Command(203);
    pub const CommandATAN: Command = Command(204);
    pub const CommandPOWE: Command = Command(205);
    pub const CommandASINH: Command = Command(206);
    pub const CommandACOSH: Command = Command(207);
    pub const CommandATANH: Command = Command(208);

    pub const ModeProgrammer: Command = Command(209);
    pub const CommandHex: Command = Command(313);
    pub const CommandDec: Command = Command(314);
    pub const CommandOct: Command = Command(315);
    pub const CommandBin: Command = Command(316);
    pub const CommandQword: Command = Command(317);
    pub const CommandDword: Command = Command(318);
    pub const CommandWord: Command = Command(319);
    pub const CommandByte: Command = Command(320);

    pub const CommandBINEDITSTART: Command = Command(700);
    pub const CommandBINPOS0: Command = Command(700);
    pub const CommandBINPOS1: Command = Command(701);
    pub const CommandBINPOS2: Command = Command(702);
    pub const CommandBINPOS3: Command = Command(703);
    pub const CommandBINPOS4: Command = Command(704);
    pub const CommandBINPOS5: Command = Command(705);
    pub const CommandBINPOS6: Command = Command(706);
    pub const CommandBINPOS7: Command = Command(707);
    pub const CommandBINPOS8: Command = Command(708);
    pub const CommandBINPOS9: Command = Command(709);
    pub const CommandBINPOS10: Command = Command(710);
    pub const CommandBINPOS11: Command = Command(711);
    pub const CommandBINPOS12: Command = Command(712);
    pub const CommandBINPOS13: Command = Command(713);
    pub const CommandBINPOS14: Command = Command(714);
    pub const CommandBINPOS15: Command = Command(715);
    pub const CommandBINPOS16: Command = Command(716);
    pub const CommandBINPOS17: Command = Command(717);
    pub const CommandBINPOS18: Command = Command(718);
    pub const CommandBINPOS19: Command = Command(719);
    pub const CommandBINPOS20: Command = Command(720);
    pub const CommandBINPOS21: Command = Command(721);
    pub const CommandBINPOS22: Command = Command(722);
    pub const CommandBINPOS23: Command = Command(723);
    pub const CommandBINPOS24: Command = Command(724);
    pub const CommandBINPOS25: Command = Command(725);
    pub const CommandBINPOS26: Command = Command(726);
    pub const CommandBINPOS27: Command = Command(727);
    pub const CommandBINPOS28: Command = Command(728);
    pub const CommandBINPOS29: Command = Command(729);
    pub const CommandBINPOS30: Command = Command(730);
    pub const CommandBINPOS31: Command = Command(731);
    pub const CommandBINPOS32: Command = Command(732);
    pub const CommandBINPOS33: Command = Command(733);
    pub const CommandBINPOS34: Command = Command(734);
    pub const CommandBINPOS35: Command = Command(735);
    pub const CommandBINPOS36: Command = Command(736);
    pub const CommandBINPOS37: Command = Command(737);
    pub const CommandBINPOS38: Command = Command(738);
    pub const CommandBINPOS39: Command = Command(739);
    pub const CommandBINPOS40: Command = Command(740);
    pub const CommandBINPOS41: Command = Command(741);
    pub const CommandBINPOS42: Command = Command(742);
    pub const CommandBINPOS43: Command = Command(743);
    pub const CommandBINPOS44: Command = Command(744);
    pub const CommandBINPOS45: Command = Command(745);
    pub const CommandBINPOS46: Command = Command(746);
    pub const CommandBINPOS47: Command = Command(747);
    pub const CommandBINPOS48: Command = Command(748);
    pub const CommandBINPOS49: Command = Command(749);
    pub const CommandBINPOS50: Command = Command(750);
    pub const CommandBINPOS51: Command = Command(751);
    pub const CommandBINPOS52: Command = Command(752);
    pub const CommandBINPOS53: Command = Command(753);
    pub const CommandBINPOS54: Command = Command(754);
    pub const CommandBINPOS55: Command = Command(755);
    pub const CommandBINPOS56: Command = Command(756);
    pub const CommandBINPOS57: Command = Command(757);
    pub const CommandBINPOS58: Command = Command(758);
    pub const CommandBINPOS59: Command = Command(759);
    pub const CommandBINPOS60: Command = Command(760);
    pub const CommandBINPOS61: Command = Command(761);
    pub const CommandBINPOS62: Command = Command(762);
    pub const CommandBINPOS63: Command = Command(763);
    pub const CommandBINEDITEND: Command = Command(763);

    /// `CommandBINPOS0 + bit` (convenience for the bit-flip buttons).
    pub const fn bin_pos(bit: u32) -> Command {
        Command(700 + bit as i32)
    }

    /// `Command0 + digit` for digits `0..=15` (convenience for digit buttons).
    pub const fn digit(d: u32) -> Command {
        Command(130 + d as i32)
    }
}

// ---------------------------------------------------------------------------
// Enums declared in CalculatorManager.h
// ---------------------------------------------------------------------------

/// `CalculationManager::CalculatorMode`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CalculatorMode {
    Standard = 0,
    Scientific,
}

/// `CalculationManager::CalculatorPrecision`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum CalculatorPrecision {
    StandardModePrecision = 16,
    ScientificModePrecision = 32,
    ProgrammerModePrecision = 64,
}

/// `CalculationManager::MemoryCommand`.
///
/// Numbering continues from the Enum Command from Command.h with some gap to
/// ensure there is no overlap of these ids when `static_cast<unsigned char>`
/// is performed on these ids they shouldn't fall in any number range greater
/// than 80. So never make the memory command ids go below 330.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum MemoryCommand {
    MemorizeNumber = 330,
    MemorizedNumberLoad = 331,
    MemorizedNumberAdd = 332,
    MemorizedNumberSubtract = 333,
    MemorizedNumberClearAll = 334,
    MemorizedNumberClear = 335,
}
