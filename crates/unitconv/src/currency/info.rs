// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Part of the Rust port of Windows Calculator's currency converter. The
// original app received "static data" (country name, currency name, symbol)
// from a Microsoft web service that no longer exists; this table supplies the
// same kind of data for the currencies the Frankfurter API publishes.

//! Display metadata for ISO 4217 currencies.
//!
//! The original currency list shows entries as `"<Country> - <Currency>"`
//! (e.g. `United States - Dollar`, `Europe - Euro`). Rate providers only
//! publish ISO codes and long names, so this table maps each code to the
//! region name, short currency name, symbol and number of fraction digits
//! (ISO 4217 minor units, used to format amounts like the original
//! `CurrencyFormatter.ApplyRoundingForCurrency`).

/// Display metadata for one currency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurrencyInfo {
    /// ISO 4217 alphabetic code, e.g. `"USD"`.
    pub code: &'static str,
    /// Country or region using the currency, e.g. `"United States"`.
    pub country_name: &'static str,
    /// Short currency name, e.g. `"Dollar"`.
    pub currency_name: &'static str,
    /// Currency symbol, e.g. `"$"`.
    pub symbol: &'static str,
    /// Number of fraction digits (ISO 4217 minor units).
    pub fraction_digits: u32,
}

/// Codes published by rate providers that are not currencies one would
/// convert between in a calculator (precious metals, SDR, regional units of
/// account, offshore quotes); these are skipped when building the unit list.
pub const EXCLUDED_CODES: &[&str] = &["XAU", "XAG", "XPD", "XPT", "XDR", "CMD", "CNH"];

/// Returns true if `code` should not be offered as a currency.
pub fn is_excluded(code: &str) -> bool {
    EXCLUDED_CODES.iter().any(|c| c.eq_ignore_ascii_case(code))
}

/// Looks up display metadata for an ISO 4217 code.
pub fn currency_info(code: &str) -> Option<&'static CurrencyInfo> {
    CURRENCIES
        .binary_search_by(|info| info.code.cmp(code))
        .ok()
        .map(|i| &CURRENCIES[i])
}

/// Number of fraction digits used to display amounts of `code`
/// (2 for unknown codes).
pub fn fraction_digits(code: &str) -> u32 {
    currency_info(code).map_or(2, |info| info.fraction_digits)
}

/// All known currencies, sorted by code.
pub fn all_currencies() -> &'static [CurrencyInfo] {
    CURRENCIES
}

macro_rules! currencies {
    ($(($code:literal, $country:literal, $name:literal, $symbol:literal, $digits:literal),)*) => {
        &[$(CurrencyInfo {
            code: $code,
            country_name: $country,
            currency_name: $name,
            symbol: $symbol,
            fraction_digits: $digits,
        },)*]
    };
}

// Sorted by code (binary searched).
static CURRENCIES: &[CurrencyInfo] = currencies![
    ("AED", "United Arab Emirates", "Dirham", "د.إ", 2),
    ("AFN", "Afghanistan", "Afghani", "؋", 2),
    ("ALL", "Albania", "Lek", "L", 2),
    ("AMD", "Armenia", "Dram", "֏", 2),
    ("ANG", "Netherlands Antilles", "Guilder", "ƒ", 2),
    ("AOA", "Angola", "Kwanza", "Kz", 2),
    ("ARS", "Argentina", "Peso", "$", 2),
    ("AUD", "Australia", "Dollar", "$", 2),
    ("AWG", "Aruba", "Florin", "ƒ", 2),
    ("AZN", "Azerbaijan", "Manat", "₼", 2),
    ("BAM", "Bosnia and Herzegovina", "Convertible Mark", "КМ", 2),
    ("BBD", "Barbados", "Dollar", "$", 2),
    ("BDT", "Bangladesh", "Taka", "৳", 2),
    ("BGN", "Bulgaria", "Lev", "лв.", 2),
    ("BHD", "Bahrain", "Dinar", "د.ب", 3),
    ("BIF", "Burundi", "Franc", "FBu", 0),
    ("BMD", "Bermuda", "Dollar", "$", 2),
    ("BND", "Brunei", "Dollar", "$", 2),
    ("BOB", "Bolivia", "Boliviano", "Bs.", 2),
    ("BRL", "Brazil", "Real", "R$", 2),
    ("BSD", "Bahamas", "Dollar", "$", 2),
    ("BTN", "Bhutan", "Ngultrum", "Nu.", 2),
    ("BWP", "Botswana", "Pula", "P", 2),
    ("BYN", "Belarus", "Ruble", "Br", 2),
    ("BZD", "Belize", "Dollar", "$", 2),
    ("CAD", "Canada", "Dollar", "$", 2),
    ("CDF", "Congo (DRC)", "Franc", "FC", 2),
    ("CHF", "Switzerland", "Franc", "CHF", 2),
    ("CLP", "Chile", "Peso", "$", 0),
    ("CNY", "China", "Yuan", "¥", 2),
    ("COP", "Colombia", "Peso", "$", 2),
    ("CRC", "Costa Rica", "Colón", "₡", 2),
    ("CUP", "Cuba", "Peso", "$", 2),
    ("CVE", "Cabo Verde", "Escudo", "$", 2),
    ("CZK", "Czechia", "Koruna", "Kč", 2),
    ("DJF", "Djibouti", "Franc", "Fdj", 0),
    ("DKK", "Denmark", "Krone", "kr.", 2),
    ("DOP", "Dominican Republic", "Peso", "$", 2),
    ("DZD", "Algeria", "Dinar", "د.ج", 2),
    ("EGP", "Egypt", "Pound", "ج.م", 2),
    ("ERN", "Eritrea", "Nakfa", "Nfk", 2),
    ("ETB", "Ethiopia", "Birr", "Br", 2),
    ("EUR", "Europe", "Euro", "€", 2),
    ("FJD", "Fiji", "Dollar", "$", 2),
    ("FKP", "Falkland Islands", "Pound", "£", 2),
    ("GBP", "United Kingdom", "Pound", "£", 2),
    ("GEL", "Georgia", "Lari", "₾", 2),
    ("GGP", "Guernsey", "Pound", "£", 2),
    ("GHS", "Ghana", "Cedi", "₵", 2),
    ("GIP", "Gibraltar", "Pound", "£", 2),
    ("GMD", "Gambia", "Dalasi", "D", 2),
    ("GNF", "Guinea", "Franc", "FG", 0),
    ("GTQ", "Guatemala", "Quetzal", "Q", 2),
    ("GYD", "Guyana", "Dollar", "$", 2),
    ("HKD", "Hong Kong SAR", "Dollar", "$", 2),
    ("HNL", "Honduras", "Lempira", "L", 2),
    ("HTG", "Haiti", "Gourde", "G", 2),
    ("HUF", "Hungary", "Forint", "Ft", 2),
    ("IDR", "Indonesia", "Rupiah", "Rp", 2),
    ("ILS", "Israel", "New Shekel", "₪", 2),
    ("IMP", "Isle of Man", "Pound", "£", 2),
    ("INR", "India", "Rupee", "₹", 2),
    ("IQD", "Iraq", "Dinar", "ع.د", 3),
    ("IRR", "Iran", "Rial", "﷼", 2),
    ("ISK", "Iceland", "Króna", "kr", 0),
    ("JEP", "Jersey", "Pound", "£", 2),
    ("JMD", "Jamaica", "Dollar", "$", 2),
    ("JOD", "Jordan", "Dinar", "د.ا", 3),
    ("JPY", "Japan", "Yen", "¥", 0),
    ("KES", "Kenya", "Shilling", "KSh", 2),
    ("KGS", "Kyrgyzstan", "Som", "сом", 2),
    ("KHR", "Cambodia", "Riel", "៛", 2),
    ("KMF", "Comoros", "Franc", "CF", 0),
    ("KPW", "North Korea", "Won", "₩", 2),
    ("KRW", "Korea", "Won", "₩", 0),
    ("KWD", "Kuwait", "Dinar", "د.ك", 3),
    ("KYD", "Cayman Islands", "Dollar", "$", 2),
    ("KZT", "Kazakhstan", "Tenge", "₸", 2),
    ("LAK", "Laos", "Kip", "₭", 2),
    ("LBP", "Lebanon", "Pound", "ل.ل", 2),
    ("LKR", "Sri Lanka", "Rupee", "Rs", 2),
    ("LRD", "Liberia", "Dollar", "$", 2),
    ("LSL", "Lesotho", "Loti", "L", 2),
    ("LYD", "Libya", "Dinar", "ل.د", 3),
    ("MAD", "Morocco", "Dirham", "د.م.", 2),
    ("MDL", "Moldova", "Leu", "L", 2),
    ("MGA", "Madagascar", "Ariary", "Ar", 2),
    ("MKD", "North Macedonia", "Denar", "ден", 2),
    ("MMK", "Myanmar", "Kyat", "K", 2),
    ("MNT", "Mongolia", "Tögrög", "₮", 2),
    ("MOP", "Macao SAR", "Pataca", "MOP$", 2),
    ("MRU", "Mauritania", "Ouguiya", "UM", 2),
    ("MUR", "Mauritius", "Rupee", "Rs", 2),
    ("MVR", "Maldives", "Rufiyaa", "Rf", 2),
    ("MWK", "Malawi", "Kwacha", "MK", 2),
    ("MXN", "Mexico", "Peso", "$", 2),
    ("MYR", "Malaysia", "Ringgit", "RM", 2),
    ("MZN", "Mozambique", "Metical", "MT", 2),
    ("NAD", "Namibia", "Dollar", "$", 2),
    ("NGN", "Nigeria", "Naira", "₦", 2),
    ("NIO", "Nicaragua", "Córdoba", "C$", 2),
    ("NOK", "Norway", "Krone", "kr", 2),
    ("NPR", "Nepal", "Rupee", "Rs", 2),
    ("NZD", "New Zealand", "Dollar", "$", 2),
    ("OMR", "Oman", "Rial", "ر.ع.", 3),
    ("PAB", "Panama", "Balboa", "B/.", 2),
    ("PEN", "Peru", "Sol", "S/", 2),
    ("PGK", "Papua New Guinea", "Kina", "K", 2),
    ("PHP", "Philippines", "Peso", "₱", 2),
    ("PKR", "Pakistan", "Rupee", "Rs", 2),
    ("PLN", "Poland", "Złoty", "zł", 2),
    ("PYG", "Paraguay", "Guaraní", "₲", 0),
    ("QAR", "Qatar", "Riyal", "ر.ق", 2),
    ("RON", "Romania", "Leu", "lei", 2),
    ("RSD", "Serbia", "Dinar", "дин.", 2),
    ("RUB", "Russia", "Ruble", "₽", 2),
    ("RWF", "Rwanda", "Franc", "FRw", 0),
    ("SAR", "Saudi Arabia", "Riyal", "ر.س", 2),
    ("SBD", "Solomon Islands", "Dollar", "$", 2),
    ("SCR", "Seychelles", "Rupee", "Rs", 2),
    ("SDG", "Sudan", "Pound", "ج.س", 2),
    ("SEK", "Sweden", "Krona", "kr", 2),
    ("SGD", "Singapore", "Dollar", "$", 2),
    ("SHP", "Saint Helena", "Pound", "£", 2),
    ("SLE", "Sierra Leone", "Leone", "Le", 2),
    ("SOS", "Somalia", "Shilling", "Sh", 2),
    ("SRD", "Suriname", "Dollar", "$", 2),
    ("SSP", "South Sudan", "Pound", "£", 2),
    ("STN", "São Tomé and Príncipe", "Dobra", "Db", 2),
    ("SVC", "El Salvador", "Colón", "₡", 2),
    ("SYP", "Syria", "Pound", "£S", 2),
    ("SZL", "Eswatini", "Lilangeni", "E", 2),
    ("THB", "Thailand", "Baht", "฿", 2),
    ("TJS", "Tajikistan", "Somoni", "ЅМ", 2),
    ("TMT", "Turkmenistan", "Manat", "m", 2),
    ("TND", "Tunisia", "Dinar", "د.ت", 3),
    ("TOP", "Tonga", "Paʻanga", "T$", 2),
    ("TRY", "Türkiye", "Lira", "₺", 2),
    ("TTD", "Trinidad and Tobago", "Dollar", "$", 2),
    ("TWD", "Taiwan", "New Dollar", "NT$", 2),
    ("TZS", "Tanzania", "Shilling", "TSh", 2),
    ("UAH", "Ukraine", "Hryvnia", "₴", 2),
    ("UGX", "Uganda", "Shilling", "USh", 0),
    ("USD", "United States", "Dollar", "$", 2),
    ("UYU", "Uruguay", "Peso", "$U", 2),
    ("UZS", "Uzbekistan", "Som", "soʻm", 2),
    ("VES", "Venezuela", "Bolívar", "Bs.", 2),
    ("VND", "Vietnam", "Dong", "₫", 0),
    ("VUV", "Vanuatu", "Vatu", "VT", 0),
    ("WST", "Samoa", "Tala", "T", 2),
    ("XAF", "Central Africa", "CFA Franc (BEAC)", "FCFA", 0),
    ("XCD", "East Caribbean", "Dollar", "$", 2),
    (
        "XCG",
        "Curaçao and Sint Maarten",
        "Caribbean Guilder",
        "Cg",
        2
    ),
    ("XOF", "West Africa", "CFA Franc (BCEAO)", "CFA", 0),
    ("XPF", "French Pacific Territories", "CFP Franc", "₣", 0),
    ("YER", "Yemen", "Rial", "﷼", 2),
    ("ZAR", "South Africa", "Rand", "R", 2),
    ("ZMW", "Zambia", "Kwacha", "K", 2),
    ("ZWG", "Zimbabwe", "Zimbabwe Gold", "ZiG", 2),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_unique() {
        for pair in CURRENCIES.windows(2) {
            assert!(
                pair[0].code < pair[1].code,
                "{} !< {}",
                pair[0].code,
                pair[1].code
            );
        }
    }

    #[test]
    fn lookups() {
        let usd = currency_info("USD").unwrap();
        assert_eq!(usd.country_name, "United States");
        assert_eq!(usd.currency_name, "Dollar");
        assert_eq!(usd.symbol, "$");
        assert_eq!(fraction_digits("JPY"), 0);
        assert_eq!(fraction_digits("JOD"), 3);
        assert_eq!(fraction_digits("EUR"), 2);
        assert_eq!(fraction_digits("???"), 2);
        assert!(is_excluded("XAU"));
        assert!(!is_excluded("USD"));
    }
}
