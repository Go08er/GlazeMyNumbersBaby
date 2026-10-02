// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! en-US resource strings used by the date calculator, copied verbatim from
//! `Calculator/Resources/en-US/Resources.resw`. Each constant names its resource key.

/// `Date_OutOfBoundMessage`
pub const DATE_OUT_OF_BOUND_MESSAGE: &str = "Date out of Bound";
/// `Date_Day`
pub const DATE_DAY: &str = "day";
/// `Date_Days`
pub const DATE_DAYS: &str = "days";
/// `Date_Month`
pub const DATE_MONTH: &str = "month";
/// `Date_Months`
pub const DATE_MONTHS: &str = "months";
/// `Date_SameDates`
pub const DATE_SAME_DATES: &str = "Same dates";
/// `Date_Week`
pub const DATE_WEEK: &str = "week";
/// `Date_Weeks`
pub const DATE_WEEKS: &str = "weeks";
/// `Date_Year`
pub const DATE_YEAR: &str = "year";
/// `Date_Years`
pub const DATE_YEARS: &str = "years";
/// `Date_DifferenceResultAutomationName` (`%1` is replaced by the difference text)
pub const DATE_DIFFERENCE_RESULT_AUTOMATION_NAME: &str = "Difference %1";
/// `Date_ResultingDateAutomationName` (`%1` is replaced by the resulting date text)
pub const DATE_RESULTING_DATE_AUTOMATION_NAME: &str = "Resulting date %1";
/// `CalculationFailed`
pub const CALCULATION_FAILED: &str = "Calculation failed";

// Labels of the date calculator page, for the GUI.

/// `DateCalculationModeText`
pub const DATE_CALCULATION_MODE_TEXT: &str = "Date calculation";
/// `DateCalculationOption.[using:Windows.UI.Xaml.Automation]AutomationProperties.Name`
pub const DATE_CALCULATION_OPTION_AUTOMATION_NAME: &str = "Calculation mode";
/// `Date_DifferenceOption.Content`
pub const DATE_DIFFERENCE_OPTION: &str = "Difference between dates";
/// `Date_AddSubtractOption.Content`
pub const DATE_ADD_SUBTRACT_OPTION: &str = "Add or subtract days";
/// `DateDiff_FromHeader.Header`
pub const DATE_DIFF_FROM_HEADER: &str = "From";
/// `DateDiff_ToHeader.Header`
pub const DATE_DIFF_TO_HEADER: &str = "To";
/// `Date_DifferenceLabel.Text`
pub const DATE_DIFFERENCE_LABEL: &str = "Difference";
/// `AddSubtract_Date_FromHeader.Header`
pub const ADD_SUBTRACT_FROM_HEADER: &str = "From";
/// `AddOption.Content`
pub const ADD_OPTION: &str = "Add";
/// `SubtractOption.Content`
pub const SUBTRACT_OPTION: &str = "Subtract";
/// `YearsLabel.Text`
pub const YEARS_LABEL: &str = "Years";
/// `MonthsLabel.Text`
pub const MONTHS_LABEL: &str = "Months";
/// `DaysLabel.Text`
pub const DAYS_LABEL: &str = "Days";
/// `DateLabel.Text`
pub const DATE_LABEL: &str = "Date";

/// `LocalizationStringUtil.GetLocalizedString(format, param)` for a single `%1` placeholder.
pub fn format_with_param(format: &str, param: &str) -> String {
    format.replace("%1", param)
}
