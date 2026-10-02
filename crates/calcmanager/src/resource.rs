// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CalculatorResource.h` (`CalculationManager::IResourceProvider`)
//! plus a default en-US provider.
//!
//! The default provider mirrors the unit-test `EngineResourceProvider`: the
//! engine strings come from `src/Calculator/Resources/en-US/CEngineStrings.resw`
//! (embedded verbatim in [`EN_US_ENGINE_STRINGS`]) and the number separators
//! default to the en-US values (`sDecimal` = `.`, `sThousand` = `,`,
//! `sGrouping` = `3;0`). Keys that are absent from the resource file resolve
//! to an empty string, exactly like `ResourceLoader::GetString`.

/// `CalculationManager::IResourceProvider`
pub trait ResourceProvider {
    /// Should return a string from the resource table for strings used
    /// by the calculation engine. The strings that must be defined
    /// and the ids to define them with can be seen in EngineStrings.h
    /// with SIDS prefix. Additionally it must provide values for string
    /// ids "sDecimal", "sThousand" and "sGrouping". See
    /// <https://technet.microsoft.com/en-us/library/cc782655(v=ws.10).aspx>
    /// for what these values refer to.
    fn get_cengine_string(&self, id: &str) -> String;
}

/// The en-US `CEngineStrings.resw` table (resource key, value), in file order.
///
/// Note: key `"23"` really is `cis` in the shipping resources; it is never
/// displayed because `IDC_COS` is always rendered through the angle-specific
/// strings (`cos₀`, `cosᵣ`, `cos₉`).
pub const EN_US_ENGINE_STRINGS: &[(&str, &str)] = &[
    ("10", "Rsh"),
    ("100", "Invalid input"),
    ("101", "Result is undefined"),
    ("105", "Not enough memory"),
    ("107", "Overflow"),
    ("108", "Result not defined"),
    ("11", "÷"),
    ("118", "Result not defined"),
    ("119", "Overflow"),
    ("12", "×"),
    ("120", "Overflow"),
    ("13", "+"),
    ("14", "-"),
    ("15", "Mod"),
    ("16", "yroot"),
    ("17", "^"),
    ("18", "Int"),
    ("19", "RoL"),
    ("2", "CE"),
    ("20", "RoR"),
    ("21", "NOT"),
    ("22", "sin"),
    ("23", "cis"),
    ("24", "tan"),
    ("25", "sinh"),
    ("26", "cosh"),
    ("27", "tanh"),
    ("28", "ln"),
    ("29", "log"),
    ("30", "√"),
    ("35", "dms"),
    ("37", "10^"),
    ("38", "%"),
    ("4", "."),
    ("40", "Pi"),
    ("41", "="),
    ("47", "Exp"),
    ("48", "("),
    ("49", ")"),
    ("6", "AND"),
    ("66", "frac"),
    ("67", "sin₀"),
    ("68", "cos₀"),
    ("69", "tan₀"),
    ("7", "OR"),
    ("70", "sin₀⁻¹"),
    ("71", "cos₀⁻¹"),
    ("72", "tan₀⁻¹"),
    ("73", "sinᵣ"),
    ("74", "cosᵣ"),
    ("75", "tanᵣ"),
    ("76", "sinᵣ⁻¹"),
    ("77", "cosᵣ⁻¹"),
    ("78", "tanᵣ⁻¹"),
    ("79", "sin₉"),
    ("8", "XOR"),
    ("80", "cos₉"),
    ("81", "tan₉"),
    ("82", "sin₉⁻¹"),
    ("83", "cos₉⁻¹"),
    ("84", "tan₉⁻¹"),
    ("85", "sinh⁻¹"),
    ("86", "cosh⁻¹"),
    ("87", "tanh⁻¹"),
    ("88", "e^"),
    ("89", "10^"),
    ("9", "Lsh"),
    ("90", "√"),
    ("91", "sqr"),
    ("92", "cube"),
    ("94", "fact"),
    ("95", "1/"),
    ("96", "degrees"),
    ("97", "negate"),
    ("99", "Cannot divide by zero"),
    ("SecDeg", "sec₀"),
    ("SecRad", "secᵣ"),
    ("SecGrad", "sec₉"),
    ("InverseSecDeg", "sec₀⁻¹"),
    ("InverseSecRad", "secᵣ⁻¹"),
    ("InverseSecGrad", "sec₉⁻¹"),
    ("CscDeg", "csc₀"),
    ("CscRad", "cscᵣ"),
    ("CscGrad", "csc₉"),
    ("InverseCscDeg", "csc₀⁻¹"),
    ("InverseCscRad", "cscᵣ⁻¹"),
    ("InverseCscGrad", "csc₉⁻¹"),
    ("CotDeg", "cot₀"),
    ("CotRad", "cotᵣ"),
    ("CotGrad", "cot₉"),
    ("InverseCotDeg", "cot₀⁻¹"),
    ("InverseCotRad", "cotᵣ⁻¹"),
    ("InverseCotGrad", "cot₉⁻¹"),
    ("Sech", "sech"),
    ("InverseSech", "sech⁻¹"),
    ("Csch", "csch"),
    ("InverseCsch", "csch⁻¹"),
    ("Coth", "coth"),
    ("InverseCoth", "coth⁻¹"),
    ("TwoPowX", "2^"),
    ("LogBaseY", "log base"),
    ("Abs", "abs"),
    ("Ceil", "ceil"),
    ("Floor", "floor"),
    ("Nand", "NAND"),
    ("Nor", "NOR"),
    ("CubeRoot", "cuberoot"),
    ("ProgrammerMod", "%"),
];

/// Looks a key up in [`EN_US_ENGINE_STRINGS`].
pub fn en_us_engine_string(id: &str) -> Option<&'static str> {
    EN_US_ENGINE_STRINGS
        .iter()
        .find(|(k, _)| *k == id)
        .map(|(_, v)| *v)
}

/// Default engine resource provider: en-US strings with configurable number
/// separators (so a UI can feed the user's locale settings).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineResourceProvider {
    /// Value returned for `sDecimal`.
    pub decimal_separator: String,
    /// Value returned for `sThousand`.
    pub thousands_separator: String,
    /// Value returned for `sGrouping` (Win32 grouping format, e.g. `3;0`).
    pub grouping: String,
}

impl Default for EngineResourceProvider {
    fn default() -> Self {
        EngineResourceProvider {
            decimal_separator: ".".to_string(),
            thousands_separator: ",".to_string(),
            grouping: "3;0".to_string(),
        }
    }
}

impl EngineResourceProvider {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ResourceProvider for EngineResourceProvider {
    fn get_cengine_string(&self, id: &str) -> String {
        // The unit tests force the en-US locale (see UnitTestApp), so the engine
        // number separators are fixed to their en-US values here.
        if id == "sDecimal" {
            return self.decimal_separator.clone();
        }

        if id == "sThousand" {
            return self.thousands_separator.clone();
        }

        if id == "sGrouping" {
            // CalcEngine consumes the Win32 grouping format; "3;0" groups every 3 digits.
            return self.grouping.clone();
        }

        en_us_engine_string(id).unwrap_or("").to_string()
    }
}
