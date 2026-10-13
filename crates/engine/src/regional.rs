//! Regional defaults, injected by the desktop host; headless sessions stay deterministic.

#[derive(Clone, Debug)]
pub struct RegionalFormats {
    pub number: String,
    pub currency: String,
    pub accounting: String,
    pub short_date: String,
    pub long_date: String,
    pub time: String,
}

impl Default for RegionalFormats {
    fn default() -> Self {
        Self {
            number: "0.00".into(),
            currency: "\"$\"#,##0.00".into(),
            accounting: crate::cmd::format::format_code_for("Accounting").into(),
            short_date: "m/d/yyyy".into(),
            long_date: crate::cmd::format::format_code_for("Long Date").into(),
            time: crate::cmd::format::format_code_for("Time").into(),
        }
    }
}

impl RegionalFormats {
    pub fn code_for<'a>(&'a self, name: &'a str) -> &'a str {
        match name {
            "Number" => &self.number,
            "Currency" => &self.currency,
            "Accounting" => &self.accounting,
            "Short Date" => &self.short_date,
            "Long Date" => &self.long_date,
            "Time" => &self.time,
            _ => crate::cmd::format::format_code_for(name),
        }
    }
}
