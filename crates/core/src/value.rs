//! Part values: what is printed or coded on a placed part, such as `10 kΩ`,
//! `100 nF` or `1N4148`.
//!
//! A definition's [`ValueKind`] says how its parts' values are read.
//! Resistances and capacitances are parsed and stored in one display form
//! (`4k7` → `4.7 kΩ`, `104` → `100 nF`), so the stored text is what every view
//! shows. Resistances also get their colour code: five bands at 1% tolerance.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// How a definition's parts read their value.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    /// Free text, shown as typed: `1N4148`, `NE555`, `red`.
    #[default]
    Text,
    /// Ohms: `4k7`, `4.7k`, `4700`, `4R7`, `2M2`, `0`.
    Resistance,
    /// Farads: `100n`, `0.1uF`, `4.7µ`, or a three-digit code such as `104`.
    Capacitance,
}

impl ValueKind {
    #[must_use]
    #[allow(clippy::trivially_copy_pass_by_ref)] // Serde's skip predicate receives a reference.
    pub const fn is_text(&self) -> bool {
        matches!(self, Self::Text)
    }
}

/// One colour band of a resistor colour code.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Black,
    Brown,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Violet,
    Grey,
    White,
    Gold,
    Silver,
}

impl Band {
    const DIGITS: [Self; 10] = [
        Self::Black,
        Self::Brown,
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Violet,
        Self::Grey,
        Self::White,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::Brown => "brown",
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Violet => "violet",
            Self::Grey => "grey",
            Self::White => "white",
            Self::Gold => "gold",
            Self::Silver => "silver",
        }
    }
}

/// The tolerance band drawn on every resistor: brown, 1%.
pub const DEFAULT_TOLERANCE: Band = Band::Brown;
/// The tolerance the default band stands for, as shown to the user.
pub const DEFAULT_TOLERANCE_TEXT: &str = "1%";
/// The longest value, in characters.
pub const MAX_VALUE_LEN: usize = 32;
/// The most significant digits a resistance or capacitance may have.
const MAX_SIGNIFICANT_DIGITS: u32 = 6;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ValueError {
    #[error("a value may be at most {MAX_VALUE_LEN} characters")]
    TooLong,
    #[error("a value must be a single line")]
    NotSingleLine,
    #[error("`{0}` is not a resistance; write it like 4k7, 4.7k, 4700 or 4R7")]
    InvalidResistance(String),
    #[error("`{0}` is not a capacitance; write it like 100n, 0.1u, 22p or the code 104")]
    InvalidCapacitance(String),
    #[error("`{0}` has a lowercase m (milli); write M for megohms, such as 2M2")]
    MilliOhms(String),
    #[error(
        "`{0}` needs a unit, such as 100n or 100p (a bare three-digit number is a code: 104 = 100 nF)"
    )]
    CapacitanceWithoutUnit(String),
    #[error("`{0}` is out of range for a {1}")]
    OutOfRange(String, &'static str),
    #[error("`{0}` has more than {MAX_SIGNIFICANT_DIGITS} significant digits")]
    TooPrecise(String),
}

/// How a stored value reads: the text to show, and for resistances the
/// colour code and for capacitances the three-digit code, when there is one.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValueReading {
    pub display: String,
    /// Five bands (digit, digit, digit, multiplier, tolerance), or one black
    /// band for a zero-ohm link. Empty when the value has no colour code.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bands: Vec<Band>,
    /// The three-digit code printed on small capacitors, such as `104`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Checks a value typed for a part of `kind` and returns the form to store:
/// trimmed text, or the normalised resistance or capacitance. An empty value
/// clears it.
///
/// # Errors
/// Returns why the value cannot be read as `kind`.
pub fn normalize_value(kind: ValueKind, input: &str) -> Result<String, ValueError> {
    let text = input.trim();
    if text.contains(['\n', '\r']) {
        return Err(ValueError::NotSingleLine);
    }
    if text.chars().count() > MAX_VALUE_LEN {
        return Err(ValueError::TooLong);
    }
    if text.is_empty() {
        return Ok(String::new());
    }
    match kind {
        ValueKind::Text => Ok(text.to_owned()),
        ValueKind::Resistance => parse_resistance(text).map(format_resistance),
        ValueKind::Capacitance => parse_capacitance(text).map(format_capacitance),
    }
}

/// Reads a stored value. Returns `None` when there is none. A value that
/// does not parse as `kind` (stored before the definition said what it is)
/// is shown as it is, without a code.
#[must_use]
pub fn read_value(kind: ValueKind, stored: &str) -> Option<ValueReading> {
    let text = stored.trim();
    if text.is_empty() {
        return None;
    }
    let plain = || ValueReading {
        display: text.to_owned(),
        bands: Vec::new(),
        code: None,
    };
    Some(match kind {
        ValueKind::Text => plain(),
        ValueKind::Resistance => parse_resistance(text).map_or_else(
            |_| plain(),
            |value| ValueReading {
                display: format_resistance(value),
                bands: resistor_bands(value),
                code: None,
            },
        ),
        ValueKind::Capacitance => parse_capacitance(text).map_or_else(
            |_| plain(),
            |value| ValueReading {
                display: format_capacitance(value),
                bands: Vec::new(),
                code: capacitor_code(value),
            },
        ),
    })
}

/// Orders two stored values of `kind`: by magnitude when both read as
/// resistances or capacitances (220 Ω before 4.7 kΩ before 10 kΩ), readable
/// values before unreadable ones, otherwise by text.
#[must_use]
pub fn compare_values(kind: ValueKind, left: &str, right: &str) -> std::cmp::Ordering {
    let read = |text: &str| match kind {
        ValueKind::Text => None,
        ValueKind::Resistance => parse_resistance(text.trim()).ok(),
        ValueKind::Capacitance => parse_capacitance(text.trim()).ok(),
    };
    match (read(left), read(right)) {
        (Some(a), Some(b)) => a.cmp_magnitude(b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left.cmp(right),
    }
}

/// An exact decimal: `digits × 10^exponent`, with no trailing zeros in
/// `digits` (zero is `0 × 10^0`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Decimal {
    digits: u64,
    exponent: i32,
}

impl Decimal {
    fn new(mut digits: u64, mut exponent: i32) -> Self {
        if digits == 0 {
            return Self {
                digits: 0,
                exponent: 0,
            };
        }
        while digits.is_multiple_of(10) {
            digits /= 10;
            exponent += 1;
        }
        Self { digits, exponent }
    }

    /// Compares values exactly: zero first, then by leading power of ten,
    /// then digit by digit.
    fn cmp_magnitude(self, other: Self) -> std::cmp::Ordering {
        match (self.digits == 0, other.digits == 0) {
            (true, true) => return std::cmp::Ordering::Equal,
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            (false, false) => {}
        }
        self.magnitude().cmp(&other.magnitude()).then_with(|| {
            let width = self.digit_count().max(other.digit_count());
            let scale = |value: Self| value.digits * 10_u64.pow(width - value.digit_count());
            scale(self).cmp(&scale(other))
        })
    }

    fn digit_count(self) -> u32 {
        self.digits.checked_ilog10().unwrap_or(0) + 1
    }

    /// The power of ten of the leading digit.
    fn magnitude(self) -> i32 {
        self.exponent + i32::try_from(self.digit_count()).unwrap_or(0) - 1
    }

    /// The digits scaled to exactly `count` significant digits, with the
    /// matching exponent; `None` when that would drop a nonzero digit.
    fn with_digits(self, count: u32) -> Option<(u64, i32)> {
        let have = self.digit_count();
        if have > count {
            return None;
        }
        let pad = count - have;
        Some((
            self.digits * 10_u64.pow(pad),
            self.exponent - i32::try_from(pad).ok()?,
        ))
    }
}

/// Splits `4.7`, `4k7`, `R47` style numbers: leading digits (with at most one
/// point), one optional letter, trailing digits. A letter between digits is
/// the decimal point (RKM notation), so then the leading part has no point.
fn parse_number(text: &str) -> Option<(Decimal, Option<char>)> {
    let mut chars = text.chars().peekable();
    let mut whole = String::new();
    let mut fraction = String::new();
    let mut seen_point = false;
    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() {
            if seen_point {
                fraction.push(ch);
            } else {
                whole.push(ch);
            }
        } else if (ch == '.' || ch == ',') && !seen_point {
            seen_point = true;
        } else {
            break;
        }
        chars.next();
    }
    let letter = chars.next();
    let mut trailing = String::new();
    for ch in chars {
        if !ch.is_ascii_digit() {
            return None;
        }
        trailing.push(ch);
    }
    if !trailing.is_empty() {
        if seen_point || letter.is_none() {
            return None;
        }
        fraction = trailing;
    }
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let all = format!("{whole}{fraction}");
    let significant = all.trim_start_matches('0');
    if significant.len() > MAX_SIGNIFICANT_DIGITS as usize + 6 {
        return None;
    }
    let digits: u64 = if significant.is_empty() {
        0
    } else {
        significant.parse().ok()?
    };
    let exponent = -i32::try_from(fraction.len()).ok()?;
    Some((Decimal::new(digits, exponent), letter))
}

fn strip_suffix_ignore_case<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let split = text.len().checked_sub(suffix.len())?;
    (text.is_char_boundary(split) && text[split..].eq_ignore_ascii_case(suffix))
        .then(|| &text[..split])
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn check_precision(original: &str, value: Decimal) -> Result<Decimal, ValueError> {
    if value.digit_count() > MAX_SIGNIFICANT_DIGITS {
        return Err(ValueError::TooPrecise(original.to_owned()));
    }
    Ok(value)
}

/// Ohms.
fn parse_resistance(original: &str) -> Result<Decimal, ValueError> {
    let invalid = || ValueError::InvalidResistance(original.to_owned());
    let mut text = compact(original);
    let mut has_unit = false;
    // Greek capital omega and the ohm sign.
    for unit in ["ohms", "ohm", "\u{3a9}", "\u{2126}"] {
        if let Some(rest) = strip_suffix_ignore_case(&text, unit) {
            text = rest.to_owned();
            has_unit = true;
            break;
        }
    }
    let (number, letter) = parse_number(&text).ok_or_else(invalid)?;
    let exponent = match letter {
        None | Some('R' | 'r') => 0,
        Some('k' | 'K') => 3,
        Some('M') => 6,
        Some('G' | 'g') => 9,
        // With a unit, `470mΩ` is clearly milliohms; without one, `2m2`
        // is almost certainly a mistyped megohm.
        Some('m') if has_unit => -3,
        Some('m') => return Err(ValueError::MilliOhms(original.to_owned())),
        Some(_) => return Err(invalid()),
    };
    let value = Decimal::new(number.digits, number.exponent + exponent);
    let value = check_precision(original, value)?;
    if value.digits != 0 && !(-3..=11).contains(&value.magnitude()) {
        return Err(ValueError::OutOfRange(original.to_owned(), "resistance"));
    }
    Ok(value)
}

/// Picofarads.
fn parse_capacitance(original: &str) -> Result<Decimal, ValueError> {
    let invalid = || ValueError::InvalidCapacitance(original.to_owned());
    let mut text = compact(original);
    let has_unit = if let Some(rest) = strip_suffix_ignore_case(&text, "f") {
        text = rest.to_owned();
        true
    } else {
        false
    };
    if !has_unit && text.len() == 3 && text.bytes().all(|byte| byte.is_ascii_digit()) {
        let bytes = text.as_bytes();
        let significant = u64::from(bytes[0] - b'0') * 10 + u64::from(bytes[1] - b'0');
        let multiplier = match bytes[2] - b'0' {
            power @ 0..=6 => i32::from(power),
            8 => -2,
            9 => -1,
            _ => return Err(invalid()),
        };
        return Ok(Decimal::new(significant, multiplier));
    }
    let (number, letter) = parse_number(&text).ok_or_else(invalid)?;
    let exponent = match letter {
        Some('p' | 'P') => 0,
        Some('n' | 'N') => 3,
        Some('u' | 'U' | 'µ' | 'μ') => 6,
        Some('m') => 9,
        None if has_unit => 12,
        None => return Err(ValueError::CapacitanceWithoutUnit(original.to_owned())),
        Some(_) => return Err(invalid()),
    };
    let value = Decimal::new(number.digits, number.exponent + exponent);
    let value = check_precision(original, value)?;
    if value.digits == 0 || !(-1..=14).contains(&value.magnitude()) {
        return Err(ValueError::OutOfRange(original.to_owned(), "capacitance"));
    }
    Ok(value)
}

/// `mantissa prefix`, in engineering notation, with `exponent` the power of
/// ten of the unit the prefixes are relative to.
fn engineering(value: Decimal, prefixes: &[(i32, &str)]) -> String {
    if value.digits == 0 {
        return format!(
            "0 {}",
            prefixes
                .iter()
                .find(|(e, _)| *e == 0)
                .map_or("", |(_, p)| *p)
        );
    }
    let magnitude = value.magnitude();
    let (step, prefix) = prefixes
        .iter()
        .rev()
        .find(|(power, _)| *power <= magnitude)
        .or_else(|| prefixes.first())
        .copied()
        .unwrap_or((0, ""));
    let shift = value.exponent - step;
    let digits = value.digits.to_string();
    let mantissa = if shift >= 0 {
        format!(
            "{digits}{}",
            "0".repeat(usize::try_from(shift).unwrap_or(0))
        )
    } else {
        let point = usize::try_from(-shift).unwrap_or(0);
        if digits.len() > point {
            let (whole, fraction) = digits.split_at(digits.len() - point);
            format!("{whole}.{fraction}")
        } else {
            format!("0.{}{digits}", "0".repeat(point - digits.len()))
        }
    };
    format!("{mantissa} {prefix}")
}

fn format_resistance(value: Decimal) -> String {
    format!(
        "{}Ω",
        engineering(value, &[(-3, "m"), (0, ""), (3, "k"), (6, "M"), (9, "G")])
    )
}

/// `value` in picofarads.
fn format_capacitance(value: Decimal) -> String {
    format!(
        "{}F",
        engineering(value, &[(0, "p"), (3, "n"), (6, "µ"), (9, "m"), (12, "")])
    )
}

/// Five-band code at the default tolerance; one black band for zero ohms;
/// empty when the value cannot be coded in three digits and a multiplier
/// from silver (×0.01) to white (×10⁹).
fn resistor_bands(value: Decimal) -> Vec<Band> {
    if value.digits == 0 {
        return vec![Band::Black];
    }
    let Some((digits, exponent)) = value.with_digits(3) else {
        return Vec::new();
    };
    let multiplier = match exponent {
        -2 => Band::Silver,
        -1 => Band::Gold,
        power @ 0..=9 => Band::DIGITS[usize::try_from(power).unwrap_or(0)],
        _ => return Vec::new(),
    };
    let digit = |place: u64| Band::DIGITS[usize::try_from(digits / place % 10).unwrap_or(0)];
    vec![
        digit(100),
        digit(10),
        digit(1),
        multiplier,
        DEFAULT_TOLERANCE,
    ]
}

/// The EIA three-digit code (`104`) for `value` in picofarads, for values of
/// 10 pF and up that fit two significant digits.
fn capacitor_code(value: Decimal) -> Option<String> {
    let (digits, exponent) = value.with_digits(2)?;
    (0..=6)
        .contains(&exponent)
        .then(|| format!("{digits}{exponent}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use Band::{Black, Brown, Gold, Red, Silver, Violet, White, Yellow};

    fn resistance(input: &str) -> String {
        normalize_value(ValueKind::Resistance, input).unwrap()
    }

    fn capacitance(input: &str) -> String {
        normalize_value(ValueKind::Capacitance, input).unwrap()
    }

    fn bands(input: &str) -> Vec<Band> {
        read_value(ValueKind::Resistance, input).unwrap().bands
    }

    #[test]
    fn resistances_are_normalised() {
        for (input, expected) in [
            ("4k7", "4.7 kΩ"),
            ("4.7k", "4.7 kΩ"),
            ("4.7K", "4.7 kΩ"),
            ("4,7k", "4.7 kΩ"),
            ("4700", "4.7 kΩ"),
            ("4700 ohm", "4.7 kΩ"),
            ("4.7 kΩ", "4.7 kΩ"),
            ("4R7", "4.7 Ω"),
            ("10R", "10 Ω"),
            ("R47", "470 mΩ"),
            ("0R47", "470 mΩ"),
            ("2M2", "2.2 MΩ"),
            ("1G", "1 GΩ"),
            ("100k", "100 kΩ"),
            ("1000k", "1 MΩ"),
            ("0.001", "1 mΩ"),
            ("0", "0 Ω"),
            ("0R", "0 Ω"),
            ("  220  ", "220 Ω"),
            ("4.99k", "4.99 kΩ"),
        ] {
            assert_eq!(resistance(input), expected, "{input}");
        }
    }

    #[test]
    fn normalised_values_read_back_unchanged() {
        for input in ["4.7 kΩ", "470 mΩ", "0 Ω", "2.2 MΩ"] {
            assert_eq!(resistance(input), input);
        }
        for input in ["100 nF", "4.7 µF", "22 pF", "1 F"] {
            assert_eq!(capacitance(input), input);
        }
    }

    #[test]
    fn bad_resistances_are_rejected_with_a_reason() {
        assert_eq!(
            normalize_value(ValueKind::Resistance, "2m2"),
            Err(ValueError::MilliOhms("2m2".into()))
        );
        for input in ["abc", "4k7k", "4.7k7", "k", "4x7", "1.2.3", "4.7nF"] {
            assert!(
                matches!(
                    normalize_value(ValueKind::Resistance, input),
                    Err(ValueError::InvalidResistance(_))
                ),
                "{input}"
            );
        }
        assert!(matches!(
            normalize_value(ValueKind::Resistance, "1234567"),
            Err(ValueError::TooPrecise(_))
        ));
        assert!(matches!(
            normalize_value(ValueKind::Resistance, "1000G"),
            Err(ValueError::OutOfRange(..))
        ));
    }

    #[test]
    fn capacitances_are_normalised() {
        for (input, expected) in [
            ("100n", "100 nF"),
            ("100nF", "100 nF"),
            ("100nf", "100 nF"),
            ("0.1u", "100 nF"),
            ("0.1uF", "100 nF"),
            ("0.1 µF", "100 nF"),
            ("4.7µ", "4.7 µF"),
            ("4u7", "4.7 µF"),
            ("22p", "22 pF"),
            ("1000p", "1 nF"),
            ("1000u", "1 mF"),
            ("1F", "1 F"),
            ("104", "100 nF"),
            ("100", "10 pF"),
            ("472", "4.7 nF"),
            ("109", "1 pF"),
        ] {
            assert_eq!(capacitance(input), expected, "{input}");
        }
    }

    #[test]
    fn bad_capacitances_are_rejected_with_a_reason() {
        assert!(matches!(
            normalize_value(ValueKind::Capacitance, "10"),
            Err(ValueError::CapacitanceWithoutUnit(_))
        ));
        assert!(matches!(
            normalize_value(ValueKind::Capacitance, "4700"),
            Err(ValueError::CapacitanceWithoutUnit(_))
        ));
        for input in ["107", "10k", "abc"] {
            assert!(
                matches!(
                    normalize_value(ValueKind::Capacitance, input),
                    Err(ValueError::InvalidCapacitance(_))
                ),
                "{input}"
            );
        }
        assert!(matches!(
            normalize_value(ValueKind::Capacitance, "0n"),
            Err(ValueError::OutOfRange(..))
        ));
    }

    #[test]
    fn text_is_trimmed_and_limited() {
        assert_eq!(
            normalize_value(ValueKind::Text, "  1N4148 "),
            Ok("1N4148".into())
        );
        assert_eq!(
            normalize_value(ValueKind::Resistance, "  "),
            Ok(String::new())
        );
        assert_eq!(
            normalize_value(ValueKind::Text, &"x".repeat(33)),
            Err(ValueError::TooLong)
        );
        assert_eq!(
            normalize_value(ValueKind::Text, "a\nb"),
            Err(ValueError::NotSingleLine)
        );
    }

    #[test]
    fn resistor_colour_codes_have_five_bands_at_one_percent() {
        assert_eq!(bands("10k"), [Brown, Black, Black, Red, Brown]);
        assert_eq!(bands("4k7"), [Yellow, Violet, Black, Brown, Brown]);
        assert_eq!(bands("4.99k"), [Yellow, White, White, Brown, Brown]);
        assert_eq!(bands("220"), [Red, Red, Black, Black, Brown]);
        assert_eq!(bands("47"), [Yellow, Violet, Black, Gold, Brown]);
        assert_eq!(bands("1"), [Brown, Black, Black, Silver, Brown]);
        assert_eq!(bands("1G"), [Brown, Black, Black, Violet, Brown]);
        assert_eq!(bands("100G"), [Brown, Black, Black, White, Brown]);
        assert_eq!(bands("0"), [Black]);
    }

    #[test]
    fn values_without_a_colour_code_have_no_bands() {
        // Below 1 Ω, four significant digits, and past white.
        assert_eq!(bands("R47"), []);
        assert_eq!(bands("4.701k"), []);
        assert_eq!(bands("1000G"), []);
    }

    #[test]
    fn capacitor_codes() {
        let code = |input: &str| read_value(ValueKind::Capacitance, input).unwrap().code;
        assert_eq!(code("100n").as_deref(), Some("104"));
        assert_eq!(code("4.7n").as_deref(), Some("472"));
        assert_eq!(code("22p").as_deref(), Some("220"));
        assert_eq!(code("1p"), None);
        assert_eq!(code("100u").as_deref(), None);
        assert_eq!(code("1u").as_deref(), Some("105"));
    }

    #[test]
    fn values_order_by_magnitude() {
        use std::cmp::Ordering::{Greater, Less};
        let r = |a, b| compare_values(ValueKind::Resistance, a, b);
        assert_eq!(r("220 Ω", "4.7 kΩ"), Less);
        assert_eq!(r("4.7 kΩ", "10 kΩ"), Less);
        assert_eq!(r("4.99 kΩ", "4.7 kΩ"), Greater);
        assert_eq!(r("0 Ω", "470 mΩ"), Less);
        assert_eq!(r("10 kΩ", "about 5k"), Less);
        assert_eq!(
            compare_values(ValueKind::Capacitance, "22 pF", "100 nF"),
            Less
        );
    }

    #[test]
    fn unreadable_stored_values_are_shown_as_they_are() {
        let reading = read_value(ValueKind::Resistance, "about 10k").unwrap();
        assert_eq!(reading.display, "about 10k");
        assert_eq!(reading.bands, []);
        assert_eq!(read_value(ValueKind::Text, ""), None);
        assert_eq!(
            read_value(ValueKind::Capacitance, "104").unwrap().display,
            "100 nF"
        );
    }
}
