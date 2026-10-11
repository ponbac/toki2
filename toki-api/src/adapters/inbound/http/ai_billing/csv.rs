//! CSV projection of an admin month's bill, with original and SEK amounts.

use std::collections::HashMap;

use crate::domain::models::{
    AiAllocationBasis, AiBillingCharge, AiBillingLine, AiConvertedAmount, AiCurrency,
    AiExchangeRate, AiExchangeRateValue, AiFeeAmount, AiMonthBill, AiMonthOverview,
    AiSubscriptionMonth, AiUsdNanos, BILLING_CURRENCY,
};

/// How the CSV names usage without a project and fees without usage.
const UNASSIGNED: &str = "Unassigned";
const UNALLOCATED_OVERHEAD: &str = "Unallocated overhead";

/// The CSV columns, in order. `billable_sek` is named after the billing
/// currency (`csv_header`).
const CSV_HEADER: [&str; 22] = [
    "month",
    "project_id",
    "project",
    "developer",
    "provider",
    "billing_mode",
    "plan",
    "billable_amount",
    "billable_currency",
    BILLABLE_CONVERTED_COLUMN,
    "rate",
    "rate_source",
    "rate_provisional",
    "api_equivalent_usd",
    "unpriced_records",
    "input_tokens",
    "cache_read_tokens",
    "cache_write_tokens",
    "output_tokens",
    "total_tokens",
    "allocation_basis",
    "warning",
];

/// Replaced by `billable_` and the billing currency in lower case.
const BILLABLE_CONVERTED_COLUMN: &str = "billable_{billing currency}";

fn csv_header() -> impl Iterator<Item = String> {
    CSV_HEADER.iter().map(|cell| {
        if *cell == BILLABLE_CONVERTED_COLUMN {
            format!("billable_{}", BILLING_CURRENCY.to_ascii_lowercase())
        } else {
            cell.to_string()
        }
    })
}

/// Renders a month's bill as RFC 4180 CSV with CRLF line ends and a UTF-8 byte
/// order mark, so spreadsheet programs read names with accents correctly.
///
/// Each row is one charge of one developer's use of one provider to one
/// project. `billable_amount` is what to bill, in `billable_currency`, and is
/// the same amount the overview shows:
///
/// - `subscription` rows bill their share of the pro-rated fee, exactly, in the
///   fee's currency. `api_equivalent_usd` is the estimate the share was weighed
///   by. A fee without usage is `Unallocated overhead` with no project.
/// - `api` rows bill their API-equivalent estimate in USD, rounded to whole
///   cents; `api_equivalent_usd` keeps it unrounded.
///
/// Summing `billable_amount` per `billable_currency` gives the overview's
/// totals. `billable_sek` is the amount in the billing currency, converted at
/// `rate` (empty for amounts already in it) as the overview converts it, and
/// sums to the overview's converted totals. `rate_source` is `riksbank` or
/// `admin`, and `rate_provisional` says whether the rate is provisional: an
/// average of the days published so far. Without a rate, the four are empty
/// and the warning says so. An
/// estimate that is wholly unknown, because every record is unpriced, is empty
/// rather than zero. Rows are ordered by project (Unassigned, then overhead,
/// last), developer, provider and billing mode. Every cell is quoted and
/// guarded against spreadsheet formula injection (`csv_cell`).
pub fn render_csv(overview: &AiMonthOverview) -> String {
    let bill: &AiMonthBill = &overview.bill;
    let names: HashMap<i32, &str> = overview
        .developers
        .iter()
        .map(|developer| (developer.user_id.as_i32(), developer.full_name.as_str()))
        .collect();
    let subscriptions: HashMap<i32, &AiSubscriptionMonth> = bill
        .subscriptions
        .iter()
        .map(|month| (month.subscription.id.as_i32(), month))
        .collect();

    let mut lines: Vec<(&AiBillingLine, &AiConvertedAmount)> =
        bill.lines.iter().zip(&overview.converted.lines).collect();
    let project_rank = |line: &AiBillingLine| match (&line.charge, &line.project) {
        (AiBillingCharge::UnallocatedOverhead { .. }, _) => 2,
        (_, None) => 1,
        (_, Some(_)) => 0,
    };
    let developer = |line: &AiBillingLine| names.get(&line.user_id.as_i32()).copied().unwrap_or("");
    lines.sort_by(|(a, _), (b, _)| {
        project_rank(a)
            .cmp(&project_rank(b))
            .then_with(|| {
                let name = |line: &AiBillingLine| {
                    line.project
                        .as_ref()
                        .map(|project| (project.name.clone(), project.id.to_string()))
                };
                name(a).cmp(&name(b))
            })
            .then_with(|| developer(a).cmp(developer(b)))
            .then_with(|| a.user_id.as_i32().cmp(&b.user_id.as_i32()))
            .then_with(|| a.provider.as_str().cmp(b.provider.as_str()))
            .then_with(|| {
                let mode = |line: &AiBillingLine| matches!(line.charge, AiBillingCharge::Api);
                mode(a).cmp(&mode(b))
            })
    });

    let mut csv = String::from("\u{feff}");
    push_row(&mut csv, csv_header());
    for (line, converted) in lines {
        let subscription = match &line.charge {
            AiBillingCharge::Api => None,
            AiBillingCharge::Subscription {
                subscription_id, ..
            }
            | AiBillingCharge::UnallocatedOverhead {
                subscription_id, ..
            } => subscriptions.get(&subscription_id.as_i32()).copied(),
        };
        let (project_id, project) = match (&line.charge, &line.project) {
            (AiBillingCharge::UnallocatedOverhead { .. }, _) => {
                (String::new(), UNALLOCATED_OVERHEAD.to_string())
            }
            (_, Some(project)) => (project.id.to_string(), project.name.clone()),
            (_, None) => (String::new(), UNASSIGNED.to_string()),
        };
        let usage = &line.usage;
        let unknown_cost = usage.priced_cost == AiUsdNanos::ZERO && usage.unpriced_records > 0;
        let billing_mode = match &line.charge {
            AiBillingCharge::Api => "api",
            _ => "subscription",
        };
        let billable = line.billable();
        let (billable_amount, billable_currency) = match (&line.charge, &billable) {
            (_, Some(amount)) => (amount.to_decimal(), amount.currency.as_str().to_string()),
            (AiBillingCharge::Api, None) => (String::new(), "USD".to_string()),
            (_, None) => (String::new(), String::new()),
        };
        let basis = subscription.and_then(|month| month.basis);
        let rate = converted.rate();

        push_row(
            &mut csv,
            [
                bill.month.to_string(),
                project_id,
                project,
                developer(line).to_string(),
                line.provider.as_str().to_string(),
                billing_mode.to_string(),
                subscription
                    .map(|month| month.subscription.terms.plan.as_str().to_string())
                    .unwrap_or_default(),
                billable_amount,
                billable_currency,
                converted
                    .amount()
                    .map(AiFeeAmount::to_decimal)
                    .unwrap_or_default(),
                rate.and_then(AiExchangeRate::rate)
                    .map(AiExchangeRateValue::to_decimal)
                    .unwrap_or_default(),
                rate.map(|rate| rate.source().to_string())
                    .unwrap_or_default(),
                rate.map(|rate| rate.is_provisional().to_string())
                    .unwrap_or_default(),
                if unknown_cost {
                    String::new()
                } else {
                    usage.priced_cost.to_decimal(4)
                },
                usage.unpriced_records.to_string(),
                usage.tokens.input.to_string(),
                usage.tokens.cache_read.to_string(),
                usage.tokens.cache_write.to_string(),
                usage.tokens.output.to_string(),
                usage.total_tokens().to_string(),
                match (&line.charge, basis) {
                    (AiBillingCharge::Api, _) => "",
                    (_, Some(AiAllocationBasis::ApiCost)) => "api_cost",
                    (_, Some(AiAllocationBasis::Tokens)) => "tokens",
                    (_, Some(AiAllocationBasis::Records)) => "records",
                    (_, None) => "none",
                }
                .to_string(),
                warning(line, converted, basis, &overview.converted.pending_rates),
            ],
        );
    }

    csv
}

/// Why a row's figures need a second look, if they do.
fn warning(
    line: &AiBillingLine,
    converted: &AiConvertedAmount,
    basis: Option<AiAllocationBasis>,
    pending: &[AiCurrency],
) -> String {
    let unpriced = line.usage.unpriced_records;
    let mut warnings = Vec::new();
    match (&line.charge, basis) {
        (AiBillingCharge::UnallocatedOverhead { .. }, _) => {
            warnings.push("no usage on the days the subscription covers".to_string());
        }
        (AiBillingCharge::Api, _) if unpriced > 0 => warnings.push(format!(
            "{unpriced} unpriced records: their cost is unknown and not billed"
        )),
        (_, Some(AiAllocationBasis::Tokens)) => {
            warnings.push("split by token share: no covered usage has a priced cost".to_string());
        }
        (_, Some(AiAllocationBasis::Records)) => warnings
            .push("split by record share: covered usage has no priced cost or tokens".to_string()),
        _ => {}
    }
    if unpriced > 0 && matches!(basis, Some(AiAllocationBasis::ApiCost)) {
        warnings.push(format!(
            "{unpriced} unpriced records: they carry no weight in the split"
        ));
    }
    match converted {
        AiConvertedAmount::MissingRate { currency } if pending.contains(currency) => {
            warnings.push(format!(
                "the {} exchange rate is still being fetched: not converted to {BILLING_CURRENCY} yet",
                currency.as_str()
            ))
        }
        AiConvertedAmount::MissingRate { currency } => warnings.push(format!(
            "no {} exchange rate for the month: not converted to {BILLING_CURRENCY}",
            currency.as_str()
        )),
        AiConvertedAmount::Converted {
            rate: Some(rate), ..
        } if rate.is_provisional() => warnings.push(format!(
            "provisional {} exchange rate: the average of the days published so far",
            rate.currency.as_str()
        )),
        _ => {}
    }

    warnings.join("; ")
}

fn push_row(csv: &mut String, cells: impl IntoIterator<Item = String>) {
    let cells: Vec<String> = cells.into_iter().map(|cell| csv_cell(&cell)).collect();
    csv.push_str(&cells.join(","));
    csv.push_str("\r\n");
}

/// Characters that make a spreadsheet read the text after them as a formula.
const FORMULA_PREFIXES: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];
/// Characters a spreadsheet may split a cell on, whatever the file's separator:
/// Swedish-locale Excel splits on `;`.
const SEPARATORS: [char; 5] = [',', ';', '\t', '\r', '\n'];

/// One CSV cell: always quoted, with quotes doubled, so no separator or line
/// break inside a value splits it. A formula prefix at the start of the value,
/// or right after a separator inside it, gets an apostrophe in front, so even
/// a program that splits the value stays on text. Amounts and counts never
/// start with one.
fn csv_cell(value: &str) -> String {
    let mut guarded = String::with_capacity(value.len() + 3);
    let mut at_start = true;
    for character in value.chars() {
        if at_start && FORMULA_PREFIXES.contains(&character) {
            guarded.push('\'');
        }
        guarded.push(character);
        at_start = SEPARATORS.contains(&character);
    }

    format!("\"{}\"", guarded.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_cells_are_quoted_and_never_start_a_formula() {
        for (value, cell) in [
            ("plain", r#""plain""#),
            ("", r#""""#),
            ("12.50", r#""12.50""#),
            ("Client A, AB", r#""Client A, AB""#),
            (r#"say "hi""#, r#""say ""hi""""#),
            ("two\nlines", "\"two\nlines\""),
            ("=1+1", r#""'=1+1""#),
            ("+46 70", r#""'+46 70""#),
            ("-5", r#""'-5""#),
            ("@SUM(A1)", r#""'@SUM(A1)""#),
            ("\tindented", "\"'\tindented\""),
            ("\rreturn", "\"'\rreturn\""),
            ("a=b", r#""a=b""#),
            // A formula after a separator, for programs that split the cell.
            ("Pro;=cmd|' /C calc'!A0", r#""Pro;'=cmd|' /C calc'!A0""#),
            ("Pro,+1", r#""Pro,'+1""#),
            ("Pro\t@x", "\"Pro\t'@x\""),
            ("Pro\n-1", "\"Pro\n'-1\""),
            ("x;;=1", r#""x;;'=1""#),
            ("=a;=b", r#""'=a;'=b""#),
            ("a;b", r#""a;b""#),
        ] {
            assert_eq!(csv_cell(value), cell, "{value:?}");
        }
    }
}
