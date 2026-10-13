//! How the page shows its estimates (`-lib/money.ts`): priced in US dollars, shown in kronor
//! by default at the month's rate, with dollars one click away.

/// The currencies the film shows estimates in.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Currency {
    Sek,
    Usd,
}

/// What one US dollar is worth in kronor: the Riksbank's average for October 2026 so far,
/// which the page named on 11 Oct.
pub const SEK_PER_USD: f32 = 10.0174;

impl Currency {
    /// What one US dollar is shown as in this currency.
    pub fn per_usd(self) -> f32 {
        match self {
            Currency::Sek => SEK_PER_USD,
            Currency::Usd => 1.0,
        }
    }
}

/// `value` in whole units with `separator` between thousands.
fn grouped(value: i64, separator: char) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(digit);
    }
    out
}

/// An amount already in `currency`, as the page writes it: `$12.34`, or `12,34 kr` the
/// Swedish way. Kronor from 100 kr read in whole kronor, `30 428 kr`.
pub fn amount(currency: Currency, amount: f32) -> String {
    let hundredths = (f64::from(amount) * 100.0).round() as i64;
    match currency {
        Currency::Usd => format!(
            "${}.{:02}",
            grouped(hundredths / 100, ','),
            hundredths % 100
        ),
        Currency::Sek if amount >= 99.995 => {
            format!("{} kr", grouped(f64::from(amount).round() as i64, ' '))
        }
        Currency::Sek => format!("{},{:02} kr", hundredths / 100, hundredths % 100),
    }
}

/// A US dollar estimate, shown in `currency`.
pub fn shown(currency: Currency, usd: f32) -> String {
    amount(currency, usd * currency.per_usd())
}

/// A chart axis value already in `currency`, in whole units: `$150`, `1 500 kr`.
pub fn axis(currency: Currency, amount: f32) -> String {
    let whole = f64::from(amount).round() as i64;
    match currency {
        Currency::Usd => format!("${}", grouped(whole, ',')),
        Currency::Sek => format!("{} kr", grouped(whole, ' ')),
    }
}

/// An exact amount of kronor from öre, such as `2 003,48 kr`.
pub fn kronor(ore: i64) -> String {
    format!("{},{:02} kr", grouped(ore / 100, ' '), ore % 100)
}

/// US dollars in öre at the film's rate, rounded half up.
pub fn usd_to_ore(usd: f32) -> i64 {
    (f64::from(usd) * f64::from(SEK_PER_USD) * 100.0).round() as i64
}
