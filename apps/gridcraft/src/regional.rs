//! Read host regional settings once at startup. No FFI/unsafe or UI-language inference.
use std::collections::HashMap;
use std::process::Command;

use gridcraft_engine::numfmt::FormatLocale;
use gridcraft_engine::regional::RegionalFormats;

pub struct Settings {
    pub formats: RegionalFormats,
    pub numbers: FormatLocale,
    pub money: FormatLocale,
}

fn output(command: &mut Command) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW: querying settings is silent.
    }
    let out = command.output().ok()?;
    if !out.status.success() || out.stdout.len() > 64 * 1024 {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

pub fn system() -> Option<Settings> {
    #[cfg(not(target_os = "windows"))]
    {
        let mut command = Command::new("locale");
        command.args(["-k", "LC_NUMERIC", "LC_MONETARY", "LC_TIME"]);
        #[cfg(target_os = "macos")]
        if let Some(tag) = output(Command::new("defaults").args(["read", "-g", "AppleLocale"])) {
            // Apple's region preference can differ from the terminal's LANG.
            let tag = tag.trim().split('@').next().unwrap_or("");
            if !tag.is_empty() {
                command.env("LC_ALL", format!("{tag}.UTF-8"));
            }
        }
        let text = output(&mut command)?;
        let settings = from_posix(&text);
        #[cfg(target_os = "macos")]
        let settings = mac_overrides(settings);
        Some(settings)
    }
    #[cfg(target_os = "windows")]
    {
        // CurrentCulture includes the user's Control Panel overrides, unlike a locale-name table.
        const SCRIPT: &str = r#"[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); $c=Get-Culture; $n=$c.NumberFormat; $d=$c.DateTimeFormat; @{decimal=$n.NumberDecimalSeparator; thousands=$n.NumberGroupSeparator; grouping=($n.NumberGroupSizes -join ';'); money_decimal=$n.CurrencyDecimalSeparator; money_thousands=$n.CurrencyGroupSeparator; money_grouping=($n.CurrencyGroupSizes -join ';'); symbol=$n.CurrencySymbol; digits=$n.CurrencyDecimalDigits; number_digits=$n.NumberDecimalDigits; number_negative=$n.NumberNegativePattern; positive=$n.CurrencyPositivePattern; negative=$n.CurrencyNegativePattern; short_date=$d.ShortDatePattern; long_date=$d.LongDatePattern; time=$d.LongTimePattern; date_separator=$d.DateSeparator; time_separator=$d.TimeSeparator; months=($d.MonthNames -join ';'); short_months=($d.AbbreviatedMonthNames -join ';'); weekdays=($d.DayNames -join ';'); short_weekdays=($d.AbbreviatedDayNames -join ';'); am=$d.AMDesignator; pm=$d.PMDesignator} | ConvertTo-Json -Compress"#;
        let text = output(Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]))?;
        from_windows(&text)
    }
}

#[cfg(target_os = "macos")]
fn mac_overrides(mut settings: Settings) -> Settings {
    // ICU symbol identifiers: decimal=0, grouping=1, currency=8,
    // monetary decimal=10, monetary grouping=17.
    if let Some(text) = output(Command::new("defaults").args(["read", "-g", "AppleICUNumberSymbols"])) {
        for (key, value) in preference_dictionary(&text) {
            match key.as_str() {
                "0" => settings.numbers.decimal = value.chars().take(8).collect(),
                "1" => settings.numbers.thousands = value.chars().take(8).collect(),
                "10" => settings.money.decimal = value.chars().take(8).collect(),
                "17" => settings.money.thousands = value.chars().take(8).collect(),
                "8" => {
                    settings.formats.currency = replace_currency(&settings.formats.currency, &value);
                    settings.formats.accounting = replace_currency(&settings.formats.accounting, &value);
                }
                _ => {}
            }
        }
    }
    if let Some(text) = output(Command::new("defaults").args(["read", "-g", "AppleICUDateFormatStrings"])) {
        for (key, value) in preference_dictionary(&text) {
            if let Some(code) = icu_date(&value) {
                match key.as_str() {
                    "1" => settings.formats.short_date = code,
                    "3" | "4" => settings.formats.long_date = code,
                    _ => {}
                }
            }
        }
    }
    settings
}

#[cfg(any(target_os = "macos", test))]
fn replace_currency(code: &str, symbol: &str) -> String {
    let replacement = symbol_code(symbol);
    if let Some(start) = code.find("[$")
        && let Some(tail) = code.get(start..)
        && let Some(end) = tail.find(']')
        && let Some(token) = tail.get(..=end)
    {
        return code.replace(token, &replacement);
    }
    code.replace("\"$\"", &replacement)
}

#[cfg(any(target_os = "macos", test))]
fn preference_dictionary(text: &str) -> Vec<(String, String)> {
    text.lines()
        .take(64)
        .filter_map(|line| {
            let (key, value) = line.trim().trim_end_matches(';').split_once('=')?;
            let key = key.trim().trim_matches('"');
            let value = value.trim().trim_matches('"');
            if value.len() > 512 || value.contains(['\\', '\n', '\r']) {
                return None;
            }
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn icu_date(pattern: &str) -> Option<String> {
    if pattern.is_empty() || pattern.len() > 512 {
        return None;
    }
    let mut chars = pattern.chars().peekable();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == '\'' {
            let mut closed = false;
            for ch in chars.by_ref() {
                if ch == '\'' {
                    closed = true;
                    break;
                }
                out.push_str(&literal(ch));
            }
            if !closed {
                return None;
            }
        } else if c.is_ascii_alphabetic() {
            let mut count = 1;
            while chars.peek() == Some(&c) {
                chars.next();
                count += 1;
            }
            match c {
                'y' => out.push_str(if count == 2 { "yy" } else { "yyyy" }),
                'M' | 'L' => out.push_str(&"m".repeat(count.min(5))),
                'd' => out.push_str(&"d".repeat(count.min(2))),
                'E' | 'e' | 'c' => out.push_str(if count <= 3 { "ddd" } else { "dddd" }),
                _ => return None,
            }
        } else {
            out.push_str(&literal(c));
        }
    }
    Some(out)
}

fn value<'a>(map: &'a HashMap<String, String>, key: &str, fallback: &'a str) -> &'a str {
    map.get(key).map(String::as_str).unwrap_or(fallback)
}
fn small_number(map: &HashMap<String, String>, key: &str, fallback: usize) -> usize {
    value(map, key, "").parse::<usize>().ok().filter(|n| *n <= 9).unwrap_or(fallback)
}
fn list(map: &HashMap<String, String>, key: &str, max: usize) -> Vec<String> {
    let text = value(map, key, "");
    if text.is_empty() {
        return Vec::new();
    }
    text.split(';').take(max).map(|s| s.chars().take(64).collect()).collect()
}
fn grouping(text: &str) -> Vec<usize> {
    // POSIX zero repeats the preceding size. 127/-1 means stop grouping.
    text.split(';').take(8).filter_map(|s| s.parse::<usize>().ok()).take_while(|n| *n != 0).collect()
}
fn symbols(map: &HashMap<String, String>, decimal: &str, thousands: &str, groups: &str) -> FormatLocale {
    FormatLocale {
        decimal: value(map, decimal, ".").chars().take(8).collect(),
        thousands: value(map, thousands, ",").chars().take(8).collect(),
        grouping: grouping(value(map, groups, "3")),
        months: list(map, "mon", 12),
        short_months: list(map, "abmon", 12),
        weekdays: list(map, "day", 7),
        short_weekdays: list(map, "abday", 7),
        am: value(map, "am_pm", "AM;PM").split(';').next().unwrap_or("AM").chars().take(64).collect(),
        pm: value(map, "am_pm", "AM;PM").split(';').nth(1).unwrap_or("PM").chars().take(64).collect(),
    }
}
fn symbol_code(symbol: &str) -> String {
    if !symbol.is_empty() && symbol.chars().count() <= 64 && !symbol.contains(['[', ']', '-', ';', '\n', '\r']) {
        format!("[${symbol}]")
    } else {
        "\"$\"".into()
    }
}
fn numeric_code(digits: usize) -> String {
    if digits == 0 { "#,##0".into() } else { format!("#,##0.{}", "0".repeat(digits.min(9))) }
}
fn accounting(positive: &str, negative: &str, zero: &str) -> String {
    format!("_({positive}_);_({negative}_);_({zero}_);_(@_)")
}

#[cfg(any(not(target_os = "windows"), test))]
fn money_pattern(symbol: &str, number: &str, before: bool, space: bool, sign_pos: usize, negative: bool) -> String {
    let space = if space { " " } else { "" };
    let body = if before { format!("{symbol}{space}{number}") } else { format!("{number}{space}{symbol}") };
    if !negative {
        return body;
    }
    match sign_pos {
        0 => format!("\\({body}\\)"),
        2 => format!("{body}-"),
        3 if before => format!("-{body}"),
        3 => format!("{number}{space}-{symbol}"),
        4 if before => format!("{symbol}-{space}{number}"),
        4 => format!("{body}-"),
        _ => format!("-{body}"),
    }
}

#[cfg(any(not(target_os = "windows"), test))]
fn from_posix(text: &str) -> Settings {
    let map: HashMap<String, String> = text
        .lines()
        .filter_map(|line| {
            let (key, val) = line.split_once('=')?;
            Some((key.to_string(), val.trim_matches('"').to_string()))
        })
        .collect();
    let mut formats = RegionalFormats { number: "#,##0.00".into(), ..Default::default() };
    if !value(&map, "currency_symbol", "").is_empty() {
        let symbol = symbol_code(value(&map, "currency_symbol", "$"));
        let digits = small_number(&map, "frac_digits", 2);
        let number = numeric_code(digits);
        let positive =
            |n: &str| money_pattern(&symbol, n, value(&map, "p_cs_precedes", "1") != "0", value(&map, "p_sep_by_space", "0") == "1", 1, false);
        let negative = |n: &str| {
            money_pattern(
                &symbol,
                n,
                value(&map, "n_cs_precedes", "1") != "0",
                value(&map, "n_sep_by_space", "0") == "1",
                small_number(&map, "n_sign_posn", 1),
                true,
            )
        };
        formats.currency = format!("{};{}", positive(&number), negative(&number));
        formats.accounting = accounting(&positive(&format!("* {number}")), &negative(&format!("* {number}")), &positive("* \"-\"??"));
    }
    if let Some(code) = posix_date(value(&map, "d_fmt", ""), false) {
        formats.short_date = code;
    }
    if let Some(code) = posix_date(value(&map, "d_t_fmt", ""), true) {
        formats.long_date = code;
    }
    if let Some(code) = posix_date(value(&map, "t_fmt", ""), false) {
        formats.time = code;
    }
    let numbers = symbols(&map, "decimal_point", "thousands_sep", "grouping");
    let mut money = symbols(&map, "mon_decimal_point", "mon_thousands_sep", "mon_grouping");
    if money.decimal.is_empty() {
        money = numbers.clone();
    }
    Settings { formats, numbers, money }
}

/// Quote literals so letters in host patterns never become spreadsheet date tokens.
fn literal(c: char) -> String {
    format!("\\{c}")
}
#[cfg(any(not(target_os = "windows"), test))]
fn posix_date(pattern: &str, date_only: bool) -> Option<String> {
    if pattern.is_empty() || pattern.len() > 512 {
        return None;
    }
    let mut result = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            result.push_str(&literal(c));
            continue;
        }
        let mut flag = chars.next()?;
        let mut unpadded = false;
        if matches!(flag, '-' | '_' | '0' | 'E' | 'O') {
            unpadded = flag == '-' || flag == '_';
            flag = chars.next()?;
        }
        let token = match flag {
            'Y' => "yyyy",
            'y' => "yy",
            'm' if unpadded => "m",
            'm' => "mm",
            'd' if unpadded => "d",
            'd' => "dd",
            'e' => "d",
            'a' => "ddd",
            'A' => "dddd",
            'b' | 'h' => "mmm",
            'B' => "mmmm",
            'H' | 'I' if !date_only => "hh",
            'M' if !date_only => "mm",
            'S' if !date_only => "ss",
            'p' if !date_only => "AM/PM",
            'R' if !date_only => "hh:mm",
            'T' | 'X' if !date_only => "hh:mm:ss",
            'r' if !date_only => "hh:mm:ss AM/PM",
            'D' => "mm/dd/yy",
            'F' => "yyyy-mm-dd",
            '%' => "\\%",
            'H' | 'I' | 'M' | 'S' | 'p' | 'R' | 'T' | 'X' | 'r' | 'Z' | 'z' if date_only => {
                // Drop separators belonging to the time, rather than leaving stray colons.
                while result.ends_with("\\:") || result.ends_with("\\ ") {
                    result.truncate(result.len().saturating_sub(2));
                }
                while chars.peek().is_some_and(|c| matches!(c, ':' | ' ')) {
                    chars.next();
                }
                result.push_str("\\ ");
                continue;
            }
            _ => return None,
        };
        result.push_str(token);
    }
    let result = result.trim_end_matches("\\ ").to_string();
    if result.is_empty() { None } else { Some(result) }
}

#[cfg(any(target_os = "windows", test))]
fn from_windows(text: &str) -> Option<Settings> {
    let data: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let obj = data.as_object()?;
    let mut map: HashMap<String, String> =
        obj.iter().map(|(key, val)| (key.clone(), val.as_str().map(str::to_string).unwrap_or_else(|| val.to_string()))).collect();
    for (from, to) in [("months", "mon"), ("short_months", "abmon"), ("weekdays", "day"), ("short_weekdays", "abday")] {
        if let Some(v) = map.get(from).cloned() {
            map.insert(to.into(), v);
        }
    }
    for key in ["grouping", "money_grouping"] {
        if let Some(value) = map.get_mut(key) {
            if value.ends_with(";0") {
                value.truncate(value.len().saturating_sub(1));
                value.push_str("127");
            } else if value == "0" {
                *value = "127".into();
            }
        }
    }
    map.insert("am_pm".into(), format!("{};{}", value(&map, "am", "AM"), value(&map, "pm", "PM")));
    let symbol = symbol_code(value(&map, "symbol", "$"));
    let number = numeric_code(small_number(&map, "digits", 2));
    let pos = ["sn", "ns", "s n", "n s"].get(small_number(&map, "positive", 0)).copied().unwrap_or("sn");
    let negatives =
        ["(sn)", "-sn", "s-n", "sn-", "(ns)", "-ns", "n-s", "ns-", "-n s", "-s n", "n s-", "s n-", "s -n", "n- s", "(s n)", "(n s)", "s- n"];
    let neg_index = value(&map, "negative", "1").parse::<usize>().unwrap_or(1);
    let neg = negatives.get(neg_index).copied().unwrap_or("-sn");
    let expand = |pattern: &str, n: &str| {
        pattern
            .chars()
            .map(|c| match c {
                's' => symbol.clone(),
                'n' => n.to_string(),
                '(' | ')' => literal(c),
                _ => c.to_string(),
            })
            .collect::<String>()
    };
    let fallback = RegionalFormats::default();
    let formats = RegionalFormats {
        number: {
            let number = numeric_code(small_number(&map, "number_digits", 2));
            let negative = ["(n)", "-n", "- n", "n-", "n -"].get(small_number(&map, "number_negative", 1)).copied().unwrap_or("-n");
            format!("{number};{}", expand(negative, &number))
        },
        currency: format!("{};{}", expand(pos, &number), expand(neg, &number)),
        accounting: accounting(&expand(pos, &format!("* {number}")), &expand(neg, &format!("* {number}")), &expand(pos, "* \"-\"??")),
        short_date: windows_date(value(&map, "short_date", ""), &map).unwrap_or(fallback.short_date),
        long_date: windows_date(value(&map, "long_date", ""), &map).unwrap_or(fallback.long_date),
        time: windows_date(value(&map, "time", ""), &map).unwrap_or(fallback.time),
    };
    Some(Settings {
        formats,
        numbers: symbols(&map, "decimal", "thousands", "grouping"),
        money: symbols(&map, "money_decimal", "money_thousands", "money_grouping"),
    })
}

#[cfg(any(target_os = "windows", test))]
fn windows_date(pattern: &str, map: &HashMap<String, String>) -> Option<String> {
    if pattern.is_empty() || pattern.len() > 512 {
        return None;
    }
    let mut chars = pattern.chars().peekable();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if matches!(c, '\'' | '"') {
            let mut closed = false;
            for ch in chars.by_ref() {
                if ch == c {
                    closed = true;
                    break;
                }
                out.push_str(&literal(ch));
            }
            if !closed {
                return None;
            }
        } else if c == '\\' {
            out.push_str(&literal(chars.next()?));
        } else if c == '/' || c == ':' {
            let separator = value(map, if c == '/' { "date_separator" } else { "time_separator" }, if c == '/' { "/" } else { ":" });
            for ch in separator.chars().take(8) {
                out.push_str(&literal(ch));
            }
        } else if matches!(c, 'd' | 'M' | 'y' | 'h' | 'H' | 'm' | 's' | 't') {
            let mut count = 1;
            while chars.peek() == Some(&c) {
                chars.next();
                count += 1;
            }
            if c == 't' {
                out.push_str("AM/PM");
            } else {
                out.push_str(&c.to_ascii_lowercase().to_string().repeat(count.min(5)));
            }
        } else if c.is_ascii_alphabetic() {
            return None;
        } else {
            out.push_str(&literal(c));
        }
    }
    Some(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use gridcraft_engine::core::{DateSystem, Value};
    use gridcraft_engine::numfmt::{NumberFormat, format_value_localized};

    fn render(settings: &Settings, name: &str, n: f64) -> String {
        let code = settings.formats.code_for(name);
        let symbols = if matches!(name, "Currency" | "Accounting") { &settings.money } else { &settings.numbers };
        format_value_localized(&Value::Number(n), &NumberFormat::parse(code), DateSystem::D1900, symbols).text
    }

    #[test]
    fn posix_uses_separate_numeric_monetary_and_date_categories() {
        let settings = from_posix(
            r#"decimal_point=","
thousands_sep="."
grouping=3;3
currency_symbol="€"
mon_decimal_point=","
mon_thousands_sep="."
mon_grouping=3;3
frac_digits=2
p_cs_precedes=0
n_cs_precedes=0
p_sep_by_space=1
n_sep_by_space=1
n_sign_posn=1
d_fmt="%d.%m.%Y"
d_t_fmt="%A %d %B %Y %H:%M:%S"
t_fmt="%H:%M:%S"
mon="Januar;Februar;März;April;Mai;Juni;Juli;August;September;Oktober;November;Dezember"
day="Sonntag;Montag;Dienstag;Mittwoch;Donnerstag;Freitag;Samstag""#,
        );
        assert_eq!(render(&settings, "Number", 1234.5), "1.234,50");
        assert_eq!(render(&settings, "Currency", 1234.5), "1.234,50 €");
        assert_eq!(render(&settings, "Currency", -1234.5), "-1.234,50 €");
        assert_eq!(render(&settings, "Short Date", 1.0), "01.01.1900");
        assert_eq!(render(&settings, "Long Date", 1.0), "Sonntag 01 Januar 1900");
        assert!(render(&settings, "Accounting", 1234.5).contains("1.234,50 €"));
        assert!(render(&settings, "Accounting", 0.0).contains("-"));
        assert_eq!(render(&settings, "Time", 0.5), "12:00:00");
    }

    #[test]
    fn mac_custom_preferences_use_icu_patterns_and_symbols() {
        let values = preference_dictionary("{\n    0 = \",\";\n    1 = \".\";\n}");
        assert_eq!(values, vec![("0".into(), ",".into()), ("1".into(), ".".into())]);
        assert_eq!(icu_date("dd/MM/y"), Some("dd\\/mm\\/yyyy".into()));
        assert_eq!(icu_date("d MMMM y"), Some("d\\ mmmm\\ yyyy".into()));
        assert_eq!(replace_currency("[$£]0.00;-[$£]0.00", "€"), "[$€]0.00;-[$€]0.00");
    }

    #[test]
    fn windows_custom_settings_are_not_inferred_from_language() {
        let settings = from_windows(r#"{"decimal":",","thousands":" ","grouping":"3","money_decimal":",","money_thousands":" ","money_grouping":"3","symbol":"£","digits":2,"positive":0,"negative":1,"short_date":"dd/MM/yyyy","long_date":"dddd, d MMMM yyyy","time":"HH:mm:ss","date_separator":"-","time_separator":":","am":"AM","pm":"PM"}"#).unwrap();
        assert_eq!(render(&settings, "Currency", 1234.5), "£1 234,50");
        assert_eq!(render(&settings, "Currency", -1234.5), "-£1 234,50");
        assert_eq!(render(&settings, "Short Date", 1.0), "01-01-1900");
        assert_eq!(render(&settings, "Time", 0.5), "12:00:00");
    }

    #[test]
    fn zero_decimal_currency_and_bad_data_fallbacks() {
        let settings = from_posix("currency_symbol=\"¥\"\nfrac_digits=0\np_cs_precedes=1\nn_sign_posn=0\nd_fmt=\"%unsupported\"");
        assert_eq!(render(&settings, "Currency", 1234.5), "¥1,235");
        assert_eq!(render(&settings, "Currency", -1234.5), "(¥1,235)");
        assert_eq!(settings.formats.short_date, RegionalFormats::default().short_date);
        assert_eq!(from_posix("").formats.currency, RegionalFormats::default().currency);
        assert_eq!(symbol_code("bad];[Red]0"), "\"$\"");
        assert_eq!(posix_date("%Q", false), None);
        assert_eq!(windows_date("yyyy g", &HashMap::new()), None);
    }
}
