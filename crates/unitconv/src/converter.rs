// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Rust port of CalcManager/UnitConverter.{h,cpp} (namespace UnitConversionManager).

//! The unit converter engine.
//!
//! [`UnitConverter`] is the double-based conversion state machine of Windows
//! Calculator: it owns the text being edited (`current display`), the
//! converted text (`return display`), the selected category and from/to
//! units, and produces "suggested" (supplementary) values. It talks to its
//! data through [`ConverterDataLoader`] implementations and reports changes
//! through [`UnitConverterVmCallback`] / [`ViewModelCurrencyCallback`].
//!
//! Most applications should drive the higher level
//! [`UnitConverterViewModel`](crate::UnitConverterViewModel) instead.

use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::currency::{CurrencyError, CurrencySnapshot, NetworkAccessBehavior};
use crate::number_formatting::{
    get_number_digits, get_number_digits_whole_number_part, round_significant_digits, stod,
    to_scientific_number, trim_trailing_zeros,
};

const EXPECTED_SERIALIZED_CATEGORY_TOKEN_COUNT: usize = 3;
const EXPECTED_SERIALIZED_UNIT_TOKEN_COUNT: usize = 6;

/// Maximum number of digits the user can enter (and that a result can have
/// before switching to scientific notation).
pub const MAXIMUM_DIGITS_ALLOWED: usize = 15;
const OPTIMAL_DIGITS_ALLOWED: u32 = 7;

const LEFT_ESCAPE_CHAR: char = '{';
const RIGHT_ESCAPE_CHAR: char = '}';

const OPTIMAL_DECIMAL_ALLOWED: f64 = 1e-6; // pow(10, -1 * (OPTIMALDIGITSALLOWED - 1))
const MINIMUM_DECIMAL_ALLOWED: f64 = 1e-14; // pow(10, -1 * (MAXIMUMDIGITSALLOWED - 1))

/// Id of [`Unit::empty`], the "null" unit.
pub const EMPTY_UNIT_ID: i32 = -1;

/// Input commands understood by [`UnitConverter::send_command`]
/// (`UnitConversionManager::Command`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    Zero,
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Decimal,
    Negate,
    Backspace,
    Clear,
    Reset,
    None,
}

impl Command {
    /// The digit command for `0..=9`.
    pub fn from_digit(digit: u32) -> Option<Command> {
        Some(match digit {
            0 => Command::Zero,
            1 => Command::One,
            2 => Command::Two,
            3 => Command::Three,
            4 => Command::Four,
            5 => Command::Five,
            6 => Command::Six,
            7 => Command::Seven,
            8 => Command::Eight,
            9 => Command::Nine,
            _ => return None,
        })
    }

    fn digit_char(self) -> Option<char> {
        Some(match self {
            Command::Zero => '0',
            Command::One => '1',
            Command::Two => '2',
            Command::Three => '3',
            Command::Four => '4',
            Command::Five => '5',
            Command::Six => '6',
            Command::Seven => '7',
            Command::Eight => '8',
            Command::Nine => '9',
            _ => return None,
        })
    }
}

/// A unit of measurement (or a currency). Equality and hashing use the id only,
/// like the C++ struct.
#[derive(Clone, Debug, Default)]
pub struct Unit {
    pub id: i32,
    pub name: String,
    pub accessible_name: String,
    pub abbreviation: String,
    pub is_conversion_source: bool,
    pub is_conversion_target: bool,
    pub is_whimsical: bool,
}

impl Unit {
    pub fn new(
        id: i32,
        name: impl Into<String>,
        abbreviation: impl Into<String>,
        is_conversion_source: bool,
        is_conversion_target: bool,
        is_whimsical: bool,
    ) -> Self {
        let name = name.into();
        Unit {
            id,
            accessible_name: name.clone(),
            name,
            abbreviation: abbreviation.into(),
            is_conversion_source,
            is_conversion_target,
            is_whimsical,
        }
    }

    /// Currency constructor: the name is `"<country> - <currency>"` (reversed
    /// for right-to-left languages) and the accessible name drops the dash.
    pub fn new_currency(
        id: i32,
        currency_name: &str,
        country_name: &str,
        abbreviation: impl Into<String>,
        is_rtl_language: bool,
        is_conversion_source: bool,
        is_conversion_target: bool,
    ) -> Self {
        let (name_value1, name_value2) = if is_rtl_language {
            (currency_name, country_name)
        } else {
            (country_name, currency_name)
        };
        Unit {
            id,
            name: format!("{name_value1} - {name_value2}"),
            accessible_name: format!("{name_value1} {name_value2}"),
            abbreviation: abbreviation.into(),
            is_conversion_source,
            is_conversion_target,
            is_whimsical: false,
        }
    }

    /// `EMPTY_UNIT`: acts as a 'null' unit.
    pub fn empty() -> Self {
        Unit::new(EMPTY_UNIT_ID, "", "", true, true, false)
    }

    pub fn is_empty(&self) -> bool {
        self.id == EMPTY_UNIT_ID
    }
}

impl PartialEq for Unit {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Unit {}

impl Hash for Unit {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// A converter category (Length, Weight, Currency, ...). Equality uses the id.
#[derive(Clone, Debug)]
pub struct Category {
    pub id: i32,
    pub name: String,
    pub supports_negative: bool,
}

impl Category {
    pub fn new(id: i32, name: impl Into<String>, supports_negative: bool) -> Self {
        Category {
            id,
            name: name.into(),
            supports_negative,
        }
    }
}

impl Default for Category {
    fn default() -> Self {
        Category {
            id: -1,
            name: String::new(),
            supports_negative: false,
        }
    }
}

impl PartialEq for Category {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Category {}

/// How to convert a value: `value * ratio + offset`, or
/// `(value + offset) * ratio` when `offset_first` is set.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ConversionData {
    pub ratio: f64,
    pub offset: f64,
    pub offset_first: bool,
}

impl ConversionData {
    pub fn new(ratio: f64, offset: f64, offset_first: bool) -> Self {
        ConversionData {
            ratio,
            offset,
            offset_first,
        }
    }

    pub fn ratio(ratio: f64) -> Self {
        ConversionData {
            ratio,
            offset: 0.0,
            offset_first: false,
        }
    }
}

/// Static data describing a currency (`CurrencyStaticData`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CurrencyStaticData {
    pub country_code: String,
    pub country_name: String,
    pub currency_code: String,
    pub currency_name: String,
    pub currency_symbol: String,
}

/// The ratios a unit converts with, in the loader's order: `(target unit, data)`.
///
/// (The C++ uses an `unordered_map<Unit, ConversionData>`; an ordered list
/// keeps supplementary results deterministic.)
pub type UnitRatios = Vec<(Unit, ConversionData)>;

/// `(units, from unit, to unit)` returned by [`UnitConverter::set_current_category`].
pub type CategorySelectionInitializer = (Vec<Unit>, Unit, Unit);

/// One supplementary ("about equal to") value: `(formatted value, unit)`.
pub type SuggestedValue = (String, Unit);

/// Receives display updates from the engine (`IUnitConverterVMCallback`).
pub trait UnitConverterVmCallback {
    fn display_callback(&mut self, from: &str, to: &str);
    fn suggested_value_callback(&mut self, suggested_values: &[SuggestedValue]);
    fn max_digits_reached(&mut self);
}

/// Receives currency related updates (`IViewModelCurrencyCallback`).
pub trait ViewModelCurrencyCallback {
    fn currency_data_load_finished(&mut self, did_load: bool);
    fn currency_symbols_callback(&mut self, from_symbol: &str, to_symbol: &str);
    fn currency_ratios_callback(&mut self, ratio_equality: &str, acc_ratio_equality: &str);
    fn currency_timestamp_callback(&mut self, timestamp: &str, is_week_old_data: bool);
    fn network_behavior_changed(&mut self, new_behavior: NetworkAccessBehavior);
}

/// Shared handle to a [`UnitConverterVmCallback`].
pub type SharedVmCallback = Arc<Mutex<dyn UnitConverterVmCallback + Send>>;
/// Shared handle to a [`ViewModelCurrencyCallback`] (shared between the engine
/// and the currency data loader, like the C++ `shared_ptr`).
pub type SharedCurrencyCallback = Arc<Mutex<dyn ViewModelCurrencyCallback + Send>>;

/// Source of categories, units and ratios (`IConverterDataLoader`).
pub trait ConverterDataLoader: Any + Send {
    /// Prepare data if necessary before calling other functions.
    fn load_data(&mut self);
    fn get_ordered_categories(&self) -> Vec<Category>;
    fn get_ordered_units(&self, category: &Category) -> Vec<Unit>;
    fn load_ordered_ratios(&self, unit: &Unit) -> UnitRatios;
    fn supports_category(&self, target: &Category) -> bool;

    /// `dynamic_pointer_cast<ICurrencyConverterDataLoader>`.
    fn as_currency_loader(&self) -> Option<&dyn CurrencyConverterDataLoader> {
        None
    }

    /// Mutable `dynamic_pointer_cast<ICurrencyConverterDataLoader>`.
    fn as_currency_loader_mut(&mut self) -> Option<&mut dyn CurrencyConverterDataLoader> {
        None
    }
}

/// Currency specific loader operations (`ICurrencyConverterDataLoader`).
///
/// Unlike the C++ interface, the web operations do not perform network I/O
/// themselves: the caller fetches (e.g. with
/// [`fetch_latest`](crate::currency::fetch_latest) on a background thread) and
/// hands the result in, so the loader never blocks.
pub trait CurrencyConverterDataLoader {
    fn set_view_model_callback(&mut self, callback: Option<SharedCurrencyCallback>);
    fn get_currency_symbols(&self, unit1: &Unit, unit2: &Unit) -> (String, String);
    fn get_currency_ratio_equality(&self, unit1: &Unit, unit2: &Unit) -> (String, String);
    fn get_currency_timestamp(&self) -> String;

    fn try_load_data_from_cache(&mut self) -> bool;
    fn try_load_data_from_web(&mut self, fetched: Result<CurrencySnapshot, CurrencyError>) -> bool;
    fn try_load_data_from_web_override(
        &mut self,
        fetched: Result<CurrencySnapshot, CurrencyError>,
    ) -> bool;
}

#[derive(Clone, Copy)]
struct SuggestedValueIntermediate<'a> {
    magnitude: f64,
    value: f64,
    unit: &'a Unit,
}

/// The unit converter engine (`UnitConversionManager::UnitConverter`).
pub struct UnitConverter {
    data_loader: Box<dyn ConverterDataLoader>,
    currency_data_loader: Option<Box<dyn ConverterDataLoader>>,
    vm_callback: Option<SharedVmCallback>,
    vm_currency_callback: Option<SharedCurrencyCallback>,
    categories: Vec<Category>,
    category_to_units: HashMap<i32, Vec<Unit>>,
    ratio_map: HashMap<i32, UnitRatios>,
    current_category: Category,
    from_type: Unit,
    to_type: Unit,
    current_display: String,
    return_display: String,
    current_has_decimal: bool,
    return_has_decimal: bool,
    switched_active: bool,
}

impl UnitConverter {
    /// Constructor, sets up all the variables and requires a data loader.
    pub fn new(data_loader: Box<dyn ConverterDataLoader>) -> Self {
        Self::with_currency_loader(data_loader, None)
    }

    /// Constructor with an additional data loader specialized for currencies.
    pub fn with_currency_loader(
        data_loader: Box<dyn ConverterDataLoader>,
        currency_data_loader: Option<Box<dyn ConverterDataLoader>>,
    ) -> Self {
        let mut converter = UnitConverter {
            data_loader,
            currency_data_loader,
            vm_callback: None,
            vm_currency_callback: None,
            categories: Vec::new(),
            category_to_units: HashMap::new(),
            ratio_map: HashMap::new(),
            current_category: Category::default(),
            from_type: Unit::empty(),
            to_type: Unit::empty(),
            current_display: String::new(),
            return_display: String::new(),
            current_has_decimal: false,
            return_has_decimal: false,
            switched_active: false,
        };
        converter.clear_values();
        converter.reset_categories_and_ratios();
        converter
    }

    /// Use to initialize first time.
    pub fn initialize(&mut self) {
        self.data_loader.load_data();
    }

    fn check_load(&mut self) -> bool {
        if self.categories.is_empty() {
            self.reset_categories_and_ratios();
        }
        !self.categories.is_empty()
    }

    /// Returns a list of the categories in use by this converter.
    pub fn get_categories(&mut self) -> Vec<Category> {
        self.check_load();
        self.categories.clone()
    }

    /// Sets the current category in use by this converter, and returns a list
    /// of unit types that exist under the given category.
    pub fn set_current_category(&mut self, input: &Category) -> CategorySelectionInitializer {
        if let Some(currency) = self.currency_data_loader.as_mut()
            && currency.supports_category(input)
        {
            currency.load_data();
        }

        let mut new_unit_list = Vec::new();
        if self.check_load() {
            if self.current_category.id != input.id {
                let (from_id, to_id) = (self.from_type.id, self.to_type.id);
                if let Some(units) = self.category_to_units.get_mut(&self.current_category.id) {
                    for unit in units.iter_mut() {
                        unit.is_conversion_source = unit.id == from_id;
                        unit.is_conversion_target = unit.id == to_id;
                    }
                }
                self.current_category = input.clone();
                if !self.current_category.supports_negative && self.current_display.starts_with('-')
                {
                    self.current_display.remove(0);
                }
            }

            new_unit_list = self.category_to_units.entry(input.id).or_default().clone();
        }

        self.initialize_selected_units();
        (new_unit_list, self.from_type.clone(), self.to_type.clone())
    }

    /// Gets the category currently being used.
    pub fn get_current_category(&self) -> Category {
        self.current_category.clone()
    }

    /// The unit currently converted from.
    pub fn from_type(&self) -> &Unit {
        &self.from_type
    }

    /// The unit currently converted to.
    pub fn to_type(&self) -> &Unit {
        &self.to_type
    }

    /// Sets the current unit types to be used, indicates a likely change in the
    /// display values, so we re-calculate and callback the updated values.
    pub fn set_current_unit_types(&mut self, from_type: &Unit, to_type: &Unit) {
        if !self.check_load() {
            return;
        }

        if self.from_type != *from_type {
            self.switched_active = true;
        }

        self.from_type = from_type.clone();
        self.to_type = to_type.clone();
        self.calculate();

        self.update_currency_symbols();
    }

    /// Switches the active field, indicating that we are now entering data
    /// into what was originally the return field, and storing results into
    /// what was originally the current field. We swap appropriate values, but
    /// do not callback, as values have not changed.
    ///
    /// `new_value` is the value the user had in the field they've just
    /// activated (the front-end may have trimmed more digits than we store).
    pub fn switch_active(&mut self, new_value: &str) {
        if !self.check_load() {
            return;
        }

        std::mem::swap(&mut self.from_type, &mut self.to_type);
        std::mem::swap(&mut self.current_has_decimal, &mut self.return_has_decimal);
        self.return_display = std::mem::replace(&mut self.current_display, new_value.to_owned());
        self.current_has_decimal = self.current_display.contains('.');
        self.switched_active = true;

        if let (Some(loader), Some(callback)) = (
            self.currency_data_loader
                .as_ref()
                .and_then(|l| l.as_currency_loader()),
            self.vm_currency_callback.as_ref(),
        ) {
            let (ratio, accessible) =
                loader.get_currency_ratio_equality(&self.from_type, &self.to_type);
            lock(callback).currency_ratios_callback(&ratio, &accessible);
        }
    }

    pub fn is_switched_active(&self) -> bool {
        self.switched_active
    }

    fn category_to_string(c: &Category, delimiter: &str) -> String {
        let mut s = Self::quote(&c.id.to_string());
        s.push_str(delimiter);
        s.push_str(&Self::quote(if c.supports_negative { "1" } else { "0" }));
        s.push_str(delimiter);
        s.push_str(&Self::quote(&c.name));
        s.push_str(delimiter);
        s
    }

    /// Splits `w` on `delimiter`. The text after the last delimiter is only
    /// included when `add_remainder` is set.
    pub fn string_to_vector(w: &str, delimiter: &str, add_remainder: bool) -> Vec<String> {
        let mut serialized_tokens = Vec::new();
        let mut start_index = 0;
        while let Some(pos) = w[start_index..].find(delimiter) {
            let delimiter_index = start_index + pos;
            serialized_tokens.push(w[start_index..delimiter_index].to_owned());
            start_index = delimiter_index + delimiter.len();
        }
        if add_remainder {
            serialized_tokens.push(w[start_index..].to_owned());
        }
        serialized_tokens
    }

    fn unit_to_string(u: &Unit, delimiter: &str) -> String {
        let flag = |b: bool| if b { "1" } else { "0" };
        let mut s = Self::quote(&u.id.to_string());
        s.push_str(delimiter);
        s.push_str(&Self::quote(&u.name));
        s.push_str(delimiter);
        s.push_str(&Self::quote(&u.abbreviation));
        s.push_str(delimiter);
        s.push_str(flag(u.is_conversion_source));
        s.push_str(delimiter);
        s.push_str(flag(u.is_conversion_target));
        s.push_str(delimiter);
        s.push_str(flag(u.is_whimsical));
        s.push_str(delimiter);
        s
    }

    fn string_to_unit(w: &str) -> Option<Unit> {
        let token_list = Self::string_to_vector(w, ";", false);
        if token_list.len() != EXPECTED_SERIALIZED_UNIT_TOKEN_COUNT {
            return None;
        }
        let name = Self::unquote(&token_list[1]);
        Some(Unit {
            id: wcstol(&Self::unquote(&token_list[0])),
            accessible_name: name.clone(),
            name,
            abbreviation: Self::unquote(&token_list[2]),
            is_conversion_source: token_list[3] == "1",
            is_conversion_target: token_list[4] == "1",
            is_whimsical: token_list[5] == "1",
        })
    }

    fn string_to_category(w: &str) -> Option<Category> {
        let token_list = Self::string_to_vector(w, ";", false);
        if token_list.len() != EXPECTED_SERIALIZED_CATEGORY_TOKEN_COUNT {
            return None;
        }
        Some(Category {
            id: wcstol(&Self::unquote(&token_list[0])),
            supports_negative: token_list[1] == "1",
            name: Self::unquote(&token_list[2]),
        })
    }

    /// De-serializes the data in the converter from a string produced by
    /// [`save_user_preferences`](Self::save_user_preferences). Malformed input
    /// is ignored.
    pub fn restore_user_preferences(&mut self, user_preferences: &str) {
        if user_preferences.is_empty() {
            return;
        }

        let outer_tokens = Self::string_to_vector(user_preferences, "|", false);
        if outer_tokens.len() != 3 {
            return;
        }

        // (The C++ asserts on malformed tokens; we ignore the whole string.)
        let (Some(from_type), Some(to_type), Some(category)) = (
            Self::string_to_unit(&outer_tokens[0]),
            Self::string_to_unit(&outer_tokens[1]),
            Self::string_to_category(&outer_tokens[2]),
        ) else {
            return;
        };
        self.current_category = category;

        // Only restore from the saved units if they are valid in the current available units.
        if let Some(cur_units) = self.category_to_units.get(&self.current_category.id) {
            if cur_units.contains(&from_type) {
                self.from_type = from_type;
            }
            if cur_units.contains(&to_type) {
                self.to_type = to_type;
            }
        }
    }

    /// Serializes the category and associated units in the converter.
    pub fn save_user_preferences(&self) -> String {
        let delimiter = ";";
        let pipe = "|";
        let mut s = Self::unit_to_string(&self.from_type, delimiter);
        s.push_str(pipe);
        s.push_str(&Self::unit_to_string(&self.to_type, delimiter));
        s.push_str(pipe);
        s.push_str(&Self::category_to_string(&self.current_category, delimiter));
        s.push_str(pipe);
        s
    }

    /// Sanitizes the input string, escape quoting any symbols we rely on for
    /// our delimiters.
    pub fn quote(s: &str) -> String {
        let mut quoted = String::with_capacity(s.len());
        for ch in s.chars() {
            match quote_conversion(ch) {
                Some(escaped) => quoted.push_str(escaped),
                None => quoted.push(ch),
            }
        }
        quoted
    }

    /// Unsanitizes a string produced by [`quote`](Self::quote).
    pub fn unquote(s: &str) -> String {
        let mut unquoted = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == LEFT_ESCAPE_CHAR {
                let mut quoted_sub_string = String::from(c);
                let mut closed = false;
                for c in chars.by_ref() {
                    quoted_sub_string.push(c);
                    if c == RIGHT_ESCAPE_CHAR {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    // Badly formatted
                    break;
                }
                // (The C++ appends L'\0' for unknown escapes; we drop them.)
                if let Some(ch) = unquote_conversion(&quoted_sub_string) {
                    unquoted.push(ch);
                }
            } else {
                unquoted.push(c);
            }
        }
        unquoted
    }

    /// Handles inputs to the converter from the view-model, corresponding to a
    /// given button or keyboard press.
    pub fn send_command(&mut self, command: Command) {
        if !self.check_load() {
            return;
        }

        // Deviation: the C++ treats `None` like a digit-less key press, which can
        // leave an empty display (and makes `stod` throw). Ignore it instead.
        if command == Command::None {
            return;
        }

        let mut clear_front;
        let mut clear_back;
        if command != Command::Negate && self.switched_active {
            self.clear_values();
            self.switched_active = false;
            clear_front = true;
            clear_back = false;
        } else {
            clear_front = self.current_display == "0";
            let len = self.current_display.len();
            clear_back = (self.current_has_decimal
                && len.saturating_sub(1) >= MAXIMUM_DIGITS_ALLOWED)
                || (!self.current_has_decimal && len >= MAXIMUM_DIGITS_ALLOWED);
        }

        if let Some(digit) = command.digit_char() {
            self.current_display.push(digit);
        } else {
            match command {
                Command::Decimal => {
                    clear_front = false;
                    clear_back = false;
                    if !self.current_has_decimal {
                        self.current_display.push('.');
                        self.current_has_decimal = true;
                    }
                }
                Command::Backspace => {
                    clear_front = false;
                    clear_back = false;
                    let len = self.current_display.len();
                    if (!self.current_display.starts_with('-') && len > 1) || len > 2 {
                        if self.current_display.ends_with('.') {
                            self.current_has_decimal = false;
                        }
                        self.current_display.pop();
                    } else {
                        self.current_display = "0".into();
                        self.current_has_decimal = false;
                    }
                }
                Command::Negate => {
                    clear_front = false;
                    clear_back = false;
                    if self.current_category.supports_negative {
                        if self.current_display.starts_with('-') {
                            self.current_display.remove(0);
                        } else {
                            self.current_display.insert(0, '-');
                        }
                    }
                }
                Command::Clear => {
                    clear_front = false;
                    clear_back = false;
                    self.clear_values();
                }
                Command::Reset => {
                    clear_front = false;
                    clear_back = false;
                    self.clear_values();
                    self.reset_categories_and_ratios();
                }
                _ => {}
            }
        }

        if clear_front && !self.current_display.is_empty() {
            self.current_display.remove(0);
        }
        if clear_back {
            self.current_display.pop();
            if let Some(callback) = &self.vm_callback {
                lock(callback).max_digits_reached();
            }
        }

        self.calculate();
    }

    /// Sets the callback interface to send display update calls to.
    pub fn set_view_model_callback(&mut self, new_callback: Option<SharedVmCallback>) {
        self.vm_callback = new_callback;
        if self.check_load() {
            self.update_view_model();
        }
    }

    /// Sets the currency callback (also forwarded to the currency data loader).
    pub fn set_view_model_currency_callback(
        &mut self,
        new_callback: Option<SharedCurrencyCallback>,
    ) {
        self.vm_currency_callback = new_callback.clone();
        if let Some(loader) = self.currency_converter_data_loader_mut() {
            loader.set_view_model_callback(new_callback);
        }
    }

    /// Hands a freshly fetched result to the currency loader as an explicit
    /// (user requested) refresh. Returns `(did_load, timestamp)`.
    ///
    /// The C++ returns a task that performs the download; here the caller
    /// downloads (see [`fetch_latest`](crate::currency::fetch_latest)).
    pub fn refresh_currency_ratios(
        &mut self,
        fetched: Result<CurrencySnapshot, CurrencyError>,
    ) -> (bool, String) {
        match self.currency_converter_data_loader_mut() {
            Some(loader) => {
                let did_load = loader.try_load_data_from_web_override(fetched);
                (did_load, loader.get_currency_timestamp())
            }
            None => (false, String::new()),
        }
    }

    /// The general (non-currency) data loader.
    pub fn data_loader(&self) -> &dyn ConverterDataLoader {
        self.data_loader.as_ref()
    }

    /// Mutable access to the general data loader.
    pub fn data_loader_mut(&mut self) -> &mut dyn ConverterDataLoader {
        self.data_loader.as_mut()
    }

    /// The currency data loader, if any.
    pub fn currency_data_loader(&self) -> Option<&dyn ConverterDataLoader> {
        self.currency_data_loader.as_deref()
    }

    /// Mutable access to the currency data loader, if any.
    pub fn currency_data_loader_mut(&mut self) -> Option<&mut (dyn ConverterDataLoader + 'static)> {
        self.currency_data_loader.as_deref_mut()
    }

    fn currency_converter_data_loader_mut(
        &mut self,
    ) -> Option<&mut dyn CurrencyConverterDataLoader> {
        self.currency_data_loader
            .as_mut()
            .and_then(|l| l.as_currency_loader_mut())
    }

    fn is_currency_category(&self, category: &Category) -> bool {
        self.currency_data_loader
            .as_ref()
            .is_some_and(|l| l.supports_category(category))
    }

    /// Converts a double value into another unit type.
    fn convert(value: f64, conversion_data: &ConversionData) -> f64 {
        if conversion_data.offset_first {
            (value + conversion_data.offset) * conversion_data.ratio
        } else {
            (value * conversion_data.ratio) + conversion_data.offset
        }
    }

    /// Calculates the suggested values for the current display value.
    fn calculate_suggested(&self) -> Vec<SuggestedValue> {
        if self.is_currency_category(&self.current_category) {
            return Vec::new();
        }

        let Some(ratios) = self.ratio_map.get(&self.from_type.id) else {
            return Vec::new();
        };

        let current_value = stod(&self.current_display);
        let mut intermediate = Vec::new();
        let mut intermediate_whimsical = Vec::new();
        // Calculate converted values for every other unit type in this category, along with their magnitude
        for (unit, data) in ratios {
            if *unit != self.from_type && *unit != self.to_type {
                let converted_value = Self::convert(current_value, data);
                let entry = SuggestedValueIntermediate {
                    magnitude: converted_value.log10(),
                    value: converted_value,
                    unit,
                };
                if unit.is_whimsical {
                    intermediate_whimsical.push(entry);
                } else {
                    intermediate.push(entry);
                }
            }
        }

        // Sort the resulting list by absolute magnitude, breaking ties by choosing the positive value
        intermediate.sort_by(compare_suggested);
        intermediate_whimsical.sort_by(compare_suggested);

        let supports_negative = self.current_category.supports_negative;
        let mut return_vector: Vec<SuggestedValue> = intermediate
            .iter()
            .filter_map(|entry| {
                let mut rounded = round_suggested(entry.value);
                if stod(&rounded) != 0.0 || supports_negative {
                    trim_trailing_zeros(&mut rounded);
                    Some((rounded, entry.unit.clone()))
                } else {
                    None
                }
            })
            .collect();

        // The whimsicals are determined differently: pick up the 'best'
        // whimsical value - currently the first non-zero one.
        let best_whimsical = intermediate_whimsical.iter().find_map(|entry| {
            let mut rounded = round_suggested(entry.value);
            if stod(&rounded) != 0.0 {
                trim_trailing_zeros(&mut rounded);
                Some((rounded, entry.unit.clone()))
            } else {
                None
            }
        });
        if let Some(whimsical) = best_whimsical {
            return_vector.push(whimsical);
        }

        return_vector
    }

    /// Resets categories and ratios (reloads them from the data loaders).
    pub fn reset_categories_and_ratios(&mut self) {
        self.switched_active = false;
        self.categories = self.data_loader.get_ordered_categories();
        if self.categories.is_empty() {
            return;
        }

        self.current_category = self.categories[0].clone();

        self.category_to_units.clear();
        self.ratio_map.clear();
        let mut ready_category_found = false;
        for category in &self.categories {
            // The data loader is different depending on the category, e.g. the
            // currency data loader is different from the static data loader.
            let active_data_loader: &dyn ConverterDataLoader = match &self.currency_data_loader {
                Some(currency) if currency.supports_category(category) => currency.as_ref(),
                _ => self.data_loader.as_ref(),
            };

            let units = active_data_loader.get_ordered_units(category);

            // Just because the units are empty, doesn't mean the user can't select this category,
            // we just want to make sure we don't let an unready category be the default.
            if !units.is_empty() {
                for u in &units {
                    self.ratio_map
                        .insert(u.id, active_data_loader.load_ordered_ratios(u));
                }

                if !ready_category_found {
                    self.current_category = category.clone();
                    ready_category_found = true;
                }
            }
            self.category_to_units.insert(category.id, units);
        }

        self.initialize_selected_units();
    }

    /// Sets the initial values for the from/to units, falling back to
    /// [`Unit::empty`] if the current category has no suitable units.
    fn initialize_selected_units(&mut self) {
        if self.category_to_units.is_empty() {
            return;
        }

        let Some(cur_units) = self.category_to_units.get(&self.current_category.id) else {
            return;
        };

        if !cur_units.is_empty() {
            // Units may already have been initialized through restore_user_preferences().
            // Check if they have been, and if so, do not override restored units.
            let is_from_unit_valid =
                !self.from_type.is_empty() && cur_units.contains(&self.from_type);
            let is_to_unit_valid = !self.to_type.is_empty() && cur_units.contains(&self.to_type);

            if is_from_unit_valid && is_to_unit_valid {
                return;
            }

            let mut conversion_source_set = false;
            let mut conversion_target_set = false;
            for cur in cur_units {
                if !conversion_source_set && cur.is_conversion_source && !is_from_unit_valid {
                    self.from_type = cur.clone();
                    conversion_source_set = true;
                }

                if !conversion_target_set && cur.is_conversion_target && !is_to_unit_valid {
                    self.to_type = cur.clone();
                    conversion_target_set = true;
                }

                if conversion_source_set && conversion_target_set {
                    return;
                }
            }
        }

        self.from_type = Unit::empty();
        self.to_type = Unit::empty();
    }

    /// Resets the value fields to 0.
    fn clear_values(&mut self) {
        self.current_has_decimal = false;
        self.return_has_decimal = false;
        self.current_display = "0".into();
    }

    fn any_unit_is_empty(&self) -> bool {
        self.from_type.is_empty() || self.to_type.is_empty()
    }

    /// Calculates a new return value based on the current display value.
    pub fn calculate(&mut self) {
        if self.any_unit_is_empty() {
            self.return_display = self.current_display.clone();
            self.return_has_decimal = self.current_has_decimal;
            trim_trailing_zeros(&mut self.return_display);
            self.update_view_model();
            return;
        }

        let conversion = self
            .ratio_map
            .get(&self.from_type.id)
            .and_then(|table| table.iter().find(|(u, _)| *u == self.to_type))
            .map(|(_, data)| *data)
            .unwrap_or_default();

        if conversion.ratio == 1.0 && conversion.offset == 0.0 {
            self.return_display = self.current_display.clone();
            self.return_has_decimal = self.current_has_decimal;
            trim_trailing_zeros(&mut self.return_display);
        } else {
            let current_value = stod(&self.current_display);
            let return_value = Self::convert(current_value, &conversion);

            if self.is_currency_category(&self.current_category) {
                // We don't need to trim the value when it's a currency.
                self.return_display =
                    round_significant_digits(return_value, MAXIMUM_DIGITS_ALLOWED as u32);
                trim_trailing_zeros(&mut self.return_display);
            } else {
                let num_pre_decimal = get_number_digits_whole_number_part(return_value);
                if num_pre_decimal > MAXIMUM_DIGITS_ALLOWED as u32
                    || (return_value != 0.0 && return_value.abs() < MINIMUM_DECIMAL_ALLOWED)
                {
                    self.return_display = to_scientific_number(return_value);
                } else {
                    let current_number_significant_digits =
                        get_number_digits(&self.current_display);
                    let precision = if return_value.abs() < OPTIMAL_DECIMAL_ALLOWED {
                        MAXIMUM_DIGITS_ALLOWED as u32
                    } else {
                        // Fewer digits are needed following the decimal if the number is large,
                        // we calculate the number of decimals necessary based on the number of digits in the integer part.
                        let number_digits = OPTIMAL_DIGITS_ALLOWED.max(
                            (MAXIMUM_DIGITS_ALLOWED as u32).min(current_number_significant_digits),
                        );
                        number_digits.saturating_sub(num_pre_decimal)
                    };

                    self.return_display = round_significant_digits(return_value, precision);
                    trim_trailing_zeros(&mut self.return_display);
                }
                self.return_has_decimal = self.return_display.contains('.');
            }
        }
        self.update_view_model();
    }

    fn update_currency_symbols(&mut self) {
        if let (Some(loader), Some(callback)) = (
            self.currency_data_loader
                .as_ref()
                .and_then(|l| l.as_currency_loader()),
            self.vm_currency_callback.as_ref(),
        ) {
            let (from_symbol, to_symbol) =
                loader.get_currency_symbols(&self.from_type, &self.to_type);
            let (ratio, accessible) =
                loader.get_currency_ratio_equality(&self.from_type, &self.to_type);

            let mut callback = lock(callback);
            callback.currency_symbols_callback(&from_symbol, &to_symbol);
            callback.currency_ratios_callback(&ratio, &accessible);
        }
    }

    fn update_view_model(&self) {
        if let Some(callback) = &self.vm_callback {
            let suggested = self.calculate_suggested();
            let mut callback = lock(callback);
            callback.display_callback(&self.current_display, &self.return_display);
            callback.suggested_value_callback(&suggested);
        }
    }

    /// The text being edited (unlocalized).
    pub fn current_display(&self) -> &str {
        &self.current_display
    }

    /// The converted text (unlocalized).
    pub fn return_display(&self) -> &str {
        &self.return_display
    }
}

fn lock<T: ?Sized>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn quote_conversion(ch: char) -> Option<&'static str> {
    Some(match ch {
        '|' => "{p}",
        '[' => "{lc}",
        ']' => "{rc}",
        ':' => "{co}",
        ',' => "{cm}",
        ';' => "{sc}",
        LEFT_ESCAPE_CHAR => "{lb}",
        RIGHT_ESCAPE_CHAR => "{rb}",
        _ => return None,
    })
}

fn unquote_conversion(s: &str) -> Option<char> {
    Some(match s {
        "{p}" => '|',
        "{lc}" => '[',
        "{rc}" => ']',
        "{co}" => ':',
        "{cm}" => ',',
        "{sc}" => ';',
        "{lb}" => LEFT_ESCAPE_CHAR,
        "{rb}" => RIGHT_ESCAPE_CHAR,
        _ => return None,
    })
}

/// `wcstol(s, nullptr, 10)`: parses a leading (optionally signed) integer, 0 if none.
fn wcstol(s: &str) -> i32 {
    let s = s.trim_start();
    let (negative, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    let value: i64 = digits[..end].parse().unwrap_or(0);
    let value = if negative { -value } else { value };
    value.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// Rounds a supplementary value: 2 decimals below 100, 1 below 1000, else 0.
fn round_suggested(value: f64) -> String {
    if value.abs() < 100.0 {
        round_significant_digits(value, 2)
    } else if value.abs() < 1000.0 {
        round_significant_digits(value, 1)
    } else {
        round_significant_digits(value, 0)
    }
}

/// Orders by absolute magnitude, breaking ties by choosing the positive value.
///
/// The C++ comparator is not a strict weak ordering when a magnitude is NaN
/// (log10 of a negative value); here NaN sorts after every number, which keeps
/// the (stable) sort well defined.
fn compare_suggested(
    first: &SuggestedValueIntermediate,
    second: &SuggestedValueIntermediate,
) -> Ordering {
    let key = |m: f64| {
        if m.is_nan() {
            (1u8, 0.0, 0.0)
        } else {
            (0u8, m.abs(), -m)
        }
    };
    let (a, b) = (key(first.magnitude), key(second.magnitude));
    a.0.cmp(&b.0)
        .then_with(|| a.1.total_cmp(&b.1))
        .then_with(|| a.2.total_cmp(&b.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_to_vector_behaviour() {
        assert_eq!(
            UnitConverter::string_to_vector("a;b;c;", ";", false),
            vec!["a", "b", "c"]
        );
        assert_eq!(
            UnitConverter::string_to_vector("a;b;c", ";", false),
            vec!["a", "b"]
        );
        assert_eq!(
            UnitConverter::string_to_vector("a;b;c", ";", true),
            vec!["a", "b", "c"]
        );
        assert_eq!(
            UnitConverter::string_to_vector("a||b|", "||", true),
            vec!["a", "b|"]
        );
    }

    #[test]
    fn wcstol_parses_prefix() {
        assert_eq!(wcstol("42"), 42);
        assert_eq!(wcstol("-7x"), -7);
        assert_eq!(wcstol("abc"), 0);
    }

    #[test]
    fn suggested_order_handles_nan() {
        let unit = Unit::empty();
        let mk = |m: f64| SuggestedValueIntermediate {
            magnitude: m,
            value: 0.0,
            unit: &unit,
        };
        let mut v = [
            mk(f64::NAN),
            mk(2.0),
            mk(-1.0),
            mk(1.0),
            mk(f64::NEG_INFINITY),
            mk(f64::NAN),
        ];
        v.sort_by(compare_suggested);
        let mags: Vec<f64> = v.iter().map(|e| e.magnitude).collect();
        assert_eq!(mags[0], 1.0);
        assert_eq!(mags[1], -1.0);
        assert_eq!(mags[2], 2.0);
        assert_eq!(mags[3], f64::NEG_INFINITY);
        assert!(mags[4].is_nan() && mags[5].is_nan());
    }
}
