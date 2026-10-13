//! Display symbols supplied by the host; format codes always use `.` and `,`.

#[derive(Clone, Debug)]
pub struct FormatLocale {
    pub decimal: String,
    pub thousands: String,
    /// Group sizes from the right, repeating the final size (e.g. [3, 2] for India).
    pub grouping: Vec<usize>,
    pub months: Vec<String>,
    pub short_months: Vec<String>,
    /// Sunday first, as in POSIX locale data.
    pub weekdays: Vec<String>,
    pub short_weekdays: Vec<String>,
    pub am: String,
    pub pm: String,
}

impl Default for FormatLocale {
    fn default() -> Self {
        Self {
            decimal: ".".into(),
            thousands: ",".into(),
            grouping: vec![3],
            months: Vec::new(),
            short_months: Vec::new(),
            weekdays: Vec::new(),
            short_weekdays: Vec::new(),
            am: "AM".into(),
            pm: "PM".into(),
        }
    }
}

impl FormatLocale {
    pub(crate) fn number(&self, text: String) -> String {
        if self.decimal == "." { text } else { text.replace('.', &self.decimal) }
    }

    pub(crate) fn group_at(&self, pos: usize) -> bool {
        if pos == 0 || self.thousands.is_empty() {
            return false;
        }
        let mut remaining = pos;
        for size in &self.grouping {
            if !(1..=9).contains(size) {
                return false;
            }
            if remaining <= *size {
                return remaining == *size;
            }
            remaining -= size;
        }
        self.grouping.last().is_some_and(|size| (1..=9).contains(size) && remaining.is_multiple_of(*size))
    }
}
