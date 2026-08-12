/// A validated local Gregorian calendar date supplied by a platform Adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CalendarDate {
    year: i32,
    month: u8,
    day: u8,
}

impl Default for CalendarDate {
    fn default() -> Self {
        Self {
            year: 2026,
            month: 1,
            day: 1,
        }
    }
}

impl CalendarDate {
    #[must_use]
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        (year >= 1 && (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month))
            .then_some(Self { year, month, day })
    }

    #[must_use]
    pub const fn year(self) -> i32 {
        self.year
    }

    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    #[must_use]
    pub const fn day(self) -> u8 {
        self.day
    }
}

/// One displayed Gregorian month with deterministic Sunday-first week cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CalendarMonth {
    year: i32,
    month: u8,
}

impl Default for CalendarMonth {
    fn default() -> Self {
        Self {
            year: 2026,
            month: 1,
        }
    }
}

impl CalendarMonth {
    #[must_use]
    pub fn new(year: i32, month: u8) -> Option<Self> {
        (year >= 1 && (1..=12).contains(&month)).then_some(Self { year, month })
    }

    #[must_use]
    pub const fn year(self) -> i32 {
        self.year
    }

    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    #[must_use]
    pub fn shifted(self, delta: i16) -> Self {
        let zero_based = self.year.saturating_mul(12) + i32::from(self.month) - 1;
        let shifted = zero_based.saturating_add(i32::from(delta)).max(0);
        Self {
            year: shifted / 12,
            month: u8::try_from(shifted % 12 + 1).unwrap_or(1),
        }
    }

    #[must_use]
    pub fn cells(self) -> [Option<u8>; 42] {
        let mut cells = [None; 42];
        let start = usize::from(weekday_sunday_zero(self.year, self.month, 1));
        for day in 1..=days_in_month(self.year, self.month) {
            cells[start + usize::from(day - 1)] = Some(day);
        }
        cells
    }
}

#[must_use]
pub const fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

const fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn weekday_sunday_zero(year: i32, month: u8, day: u8) -> u8 {
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let adjusted_year = if month < 3 { year - 1 } else { year };
    let value = adjusted_year + adjusted_year / 4 - adjusted_year / 100
        + adjusted_year / 400
        + OFFSETS[usize::from(month - 1)]
        + i32::from(day);
    u8::try_from(value.rem_euclid(7)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{CalendarDate, CalendarMonth, days_in_month};

    #[test]
    fn leap_year_and_month_shift_are_deterministic() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(
            CalendarMonth::new(2026, 1).unwrap().shifted(-1),
            CalendarMonth::new(2025, 12).unwrap()
        );
        assert_eq!(
            CalendarMonth::new(2026, 12).unwrap().shifted(1),
            CalendarMonth::new(2027, 1).unwrap()
        );
    }

    #[test]
    fn july_2026_has_42_cells_and_starts_on_wednesday() {
        let cells = CalendarMonth::new(2026, 7).unwrap().cells();
        assert_eq!(cells.len(), 42);
        assert_eq!(cells[3], Some(1));
        assert_eq!(cells[33], Some(31));
        assert!(cells[34..].iter().all(Option::is_none));
        assert_eq!(
            CalendarDate::new(2026, 7, 16).map(CalendarDate::day),
            Some(16)
        );
    }
}
