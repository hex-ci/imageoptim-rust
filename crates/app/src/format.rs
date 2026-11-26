//! Formatting helpers that match the macOS `NSByteCountFormatter` /
//! `NSNumberFormatter` output the original app uses.

/// Decimal byte count matching `NSByteCountFormatter` defaults (file style,
/// adaptive units): `17 KB`, `1.2 MB`, `845 bytes`, `Zero KB`.
/// Adaptive precision: 0 fraction digits for bytes/KB, 1 for MB, 2 for GB+.
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if n == 0 {
        return "Zero KB".to_string();
    }
    if n < 1000 {
        return if n == 1 {
            "1 byte".to_string()
        } else {
            format!("{n} bytes")
        };
    }
    let mut value = n as f64 / 1000.0;
    let mut unit = 0usize;
    loop {
        let digits: usize = match unit {
            0 => 0,
            1 => 1,
            _ => 2,
        };
        let factor = 10f64.powi(digits as i32);
        let rounded = (value * factor).round() / factor;
        if rounded >= 1000.0 && unit + 1 < UNITS.len() {
            value /= 1000.0;
            unit += 1;
            continue;
        }
        let mut text = group_digits(rounded.trunc() as u64);
        if digits > 0 {
            let frac = ((rounded - rounded.trunc()) * factor).round() as u64;
            // NSNumberFormatter: maximumFractionDigits 1/2, minimum 0 —
            // "1 MB", "1.2 MB", "1.23 GB", never "1.0 MB".
            let mut frac_text = format!("{frac:0digits$}");
            while frac_text.ends_with('0') {
                frac_text.pop();
            }
            if !frac_text.is_empty() {
                text.push_str(decimal_separator());
                text.push_str(&frac_text);
            }
        }
        return format!("{text} {}", UNITS[unit]);
    }
}

/// Plain byte count with thousands separators, as the original Size column
/// shows it (`NSByteCountFormatter` with `includesUnit = NO`).
pub fn raw_bytes(n: u64) -> String {
    group_digits(n)
}

fn group_digits(n: u64) -> String {
    let digits = n.to_string();
    let separator = thousands_separator();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push_str(separator);
        }
        out.push(ch);
    }
    out
}

/// One fraction digit at most, no trailing `.0` — `12.3%`, `27%`.
pub fn percent(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if (rounded - rounded.round()).abs() < 0.05 {
        format!("{}%", rounded.round() as i64)
    } else {
        format!("{rounded:.1}%").replace('.', decimal_separator())
    }
}

/// Number formatting follows the same locale conventions as NSNumberFormatter.
fn european(lang: &str) -> bool {
    matches!(
        lang,
        "de" | "fr"
            | "es"
            | "it"
            | "pt"
            | "nl"
            | "ru"
            | "pl"
            | "cs"
            | "sk"
            | "sv"
            | "fi"
            | "no"
            | "da"
            | "tr"
            | "uk"
            | "ro"
            | "lt"
            | "el"
            | "vi"
    )
}

fn thousands_separator() -> &'static str {
    let lang = crate::i18n::language();
    if !european(lang) {
        return ",";
    }
    match lang {
        "fr" | "ru" | "pl" | "cs" | "sk" | "sv" | "fi" | "no" | "da" | "uk" => "\u{202f}",
        _ => ".",
    }
}

fn decimal_separator() -> &'static str {
    if european(crate::i18n::language()) {
        ","
    } else {
        "."
    }
}

/// The Savings column: negative/insignificant savings read as `0%`.
pub fn savings(percent_saved: Option<f64>) -> String {
    // Improvement over upstream: tiny savings show their rounded value
    // (0.1%) instead of being forced to a misleading "0%".
    match percent_saved {
        Some(v) if v >= 0.0 => percent(v),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_formatting() {
        assert_eq!(bytes(0), "Zero KB");
        assert_eq!(bytes(1), "1 byte");
        assert_eq!(bytes(999), "999 bytes");
        assert_eq!(bytes(1000), "1 KB");
        assert_eq!(bytes(1500), "2 KB");
        assert_eq!(bytes(16_925), "17 KB");
        assert_eq!(bytes(169_000), "169 KB");
        assert_eq!(bytes(999_999), "1 MB");
        assert_eq!(bytes(1_234_567), "1.2 MB");
        assert_eq!(bytes(1_234_567_890), "1.23 GB");
        assert_eq!(raw_bytes(0), "0");
        assert_eq!(raw_bytes(112), "112");
        assert_eq!(raw_bytes(1_989), "1,989");
        assert_eq!(raw_bytes(14_814), "14,814");
    }

    #[test]
    fn percent_formatting() {
        assert_eq!(percent(0.0), "0%");
        assert_eq!(percent(12.34), "12.3%");
        assert_eq!(percent(27.0), "27%");
        assert_eq!(savings(Some(0.001)), "0%");
        assert_eq!(savings(Some(0.0568)), "0.1%");
        assert_eq!(savings(None), "");
    }
}
