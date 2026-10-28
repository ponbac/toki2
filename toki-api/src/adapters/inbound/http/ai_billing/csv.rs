//! CSV projection of an admin month's bill, with original and SEK amounts.

use std::collections::HashMap;

use crate::domain::models::{
    AiAdjustedKind, AiAdjustmentLine, AiAllocationBasis, AiBillingCharge, AiBillingLinePart,
    AiBillingMonth, AiBillingSubject, AiConvertedAmount, AiCurrency, AiExchangeRate,
    AiExchangeRateValue, AiFeeAmount, AiMeteredAccountReason, AiMeteredCharge, AiMeteredChargeKind,
    AiMeteredProvisional, AiMonthBill, AiMonthCloseId, AiMonthOverview, AiProvider,
    AiSubscriptionMonth, AiUnassignedKind, AiUsdNanos, BILLING_CURRENCY,
};
use crate::domain::AiBillingError;

/// How the CSV names usage without a project and fees without usage.
const UNASSIGNED: &str = "Unassigned";
const UNALLOCATED_OVERHEAD: &str = "Unallocated overhead";

/// The CSV columns, in order. `billable_sek` is named after the billing
/// currency (`csv_header`).
const CSV_HEADER: [&str; 32] = [
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
    // After the original columns, so those keep their places.
    "unassigned",
    "source",
    "credits",
    "github_login",
    "repository",
    "sku",
    "period_basis",
    "adjusts_month",
    "snapshot_id",
    "provisional",
];

/// Replaced by `billable_` and the billing currency in lower case.
const BILLABLE_CONVERTED_COLUMN: &str = "billable_{billing currency}";

pub(super) fn csv_header() -> impl Iterator<Item = String> {
    CSV_HEADER.iter().map(|cell| {
        if *cell == BILLABLE_CONVERTED_COLUMN {
            format!("billable_{}", BILLING_CURRENCY.to_ascii_lowercase())
        } else {
            cell.to_string()
        }
    })
}

/// Where a metered payer's numbers come from, as the `source` column names
/// it: its billing system.
fn metered_source(payer: AiProvider) -> &'static str {
    match payer {
        AiProvider::Copilot => "github",
        other => other.as_str(),
    }
}

/// A metered line's charge.
fn metered<'a>(part: &AiBillingLinePart<'a>) -> Option<&'a AiMeteredCharge> {
    match &part.line.charge {
        AiBillingCharge::Metered(charge) => Some(charge),
        _ => None,
    }
}

/// A metered payer's name in the Spinit lines' labels.
fn payer_name(payer: AiProvider) -> &'static str {
    match payer {
        AiProvider::Copilot => "Copilot",
        other => other.as_str(),
    }
}

/// A bill's CSV before rendering: the header, and the raw cells of every
/// part's row then every adjustment line's, with the order to write them in.
pub(super) struct CsvTable {
    pub header: Vec<String>,
    /// The parts' rows in the parts' order, then the adjustment lines' rows
    /// in theirs.
    pub rows: Vec<Vec<String>>,
    /// Indices into `rows` in the CSV's row order.
    pub order: Vec<usize>,
}

/// Renders a month's bill as RFC 4180 CSV with CRLF line ends and a UTF-8 byte
/// order mark, so spreadsheet programs read names with accents correctly.
///
/// Each row is one charge of one subject's use of one provider to one
/// project. `billable_amount` is what to bill, in `billable_currency`, and is
/// the same amount the overview shows. `Unassigned` usage has a row per kind,
/// named in `unassigned`: `unmapped` for repositories without a mapping,
/// `unmapped_repository` for a metered payer's credits on a repository without
/// a repository mapping (code review), and `unattributed` for usage outside
/// any repository. They split one charge in each currency, as the overview
/// does (`AiBillingLine::parts`).
///
/// - `subscription` rows bill their share of the pro-rated fee, exactly, in the
///   fee's currency. `api_equivalent_usd` is the estimate the share was weighed
///   by. A fee without usage is `Unallocated overhead` with no project.
/// - `api` rows bill their API-equivalent estimate in USD, rounded to whole
///   cents; `api_equivalent_usd` keeps it unrounded.
/// - `metered` rows bill a metered payer's credits at $0.01 (`credits`), per
///   UTC month (`period_basis`): a project's, or one of Spinit's (`Untracked
///   Copilot`, `Personal / internal Copilot`, `Unlinked account`, `External
///   account`, `Unknown account`), with the account's `github_login`, the
///   `sku` and whether they are `provisional`. `api_equivalent_usd` and the
///   tokens are the tracked usage the credits were split by. A repository's
///   code review credits name it in `repository`, with no developer or
///   login and no tracked usage.
/// - `adjustment` rows follow the month's own rows: changes to a closed month
///   (`adjusts_month`, whose close is `snapshot_id`) since it closed, posted
///   on this month, with signed amounts. Each names the project, subject
///   (`developer`, an account's `github_login` or a `repository`) and
///   provider of what changed, `source` as the changed kind's, and the rate
///   of the month it corrects; usage columns are zero. The warning names the
///   corrected month, and says when its rate changed since the key was last
///   invoiced, or when only the SEK amounts rounded differently.
///
/// `source` says where a row's numbers come from: `estimate` (API rows),
/// `subscription` (fees and overhead) or the metered payer's billing system
/// (`github`). `period_basis` is the time zone of the row's days. A closed
/// month's own rows carry its close in `snapshot_id`; an open month's own
/// rows leave both close columns empty.
///
/// Summing `billable_amount` of the own rows per `billable_currency` gives the
/// overview's totals, and of the adjustment rows `totals.adjustments`.
/// `billable_sek` is the amount in the billing currency, converted at `rate`
/// (empty for amounts already in it) as the overview converts it, and sums to
/// the overview's converted totals; an adjustment's is the difference of the
/// amounts invoiced in the billing currency, so `billable_sek = billable ×
/// rate` need not hold for it. `rate_source` is `riksbank` or `admin`, and
/// `rate_provisional` says whether the rate is provisional: an average of the
/// days published so far. Without a rate, the four are empty and the warning
/// says so. An estimate that is wholly unknown, because every record is
/// unpriced, is empty rather than zero. Own rows are ordered by project
/// (Unassigned, unmapped, then unmapped repositories, then unattributed, then
/// overhead, untracked, personal and account rows, last), developer, subject,
/// provider and billing mode. Every cell is quoted and guarded against
/// spreadsheet formula injection (`csv_cell`); numbers, negative ones
/// included, stay numbers.
pub fn render_csv(overview: &AiMonthOverview, time_zone: &str) -> Result<String, AiBillingError> {
    let parts = overview.bill.parts(&overview.converted)?;
    let table = table(overview, &parts, time_zone, None)?;
    Ok(render(
        &table.header,
        table.order.iter().map(|index| &table.rows[*index]),
    ))
}

/// The CSV text of `header` and `rows`, in order.
pub(super) fn render<'a>(header: &[String], rows: impl Iterator<Item = &'a Vec<String>>) -> String {
    let mut csv = String::from("\u{feff}");
    push_row(&mut csv, header.iter().cloned());
    for row in rows {
        push_row(&mut csv, row.iter().cloned());
    }
    csv
}

/// The rows of a month's bill (see `render_csv`). `parts` are the bill's
/// lines as parts; `close` is the month's close when the table is its
/// snapshot, which its own rows then name.
pub(super) fn table(
    overview: &AiMonthOverview,
    parts: &[AiBillingLinePart<'_>],
    time_zone: &str,
    close: Option<AiMonthCloseId>,
) -> Result<CsvTable, AiBillingError> {
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

    let project_rank = |part: &AiBillingLinePart| match (&part.line.charge, &part.line.project) {
        (AiBillingCharge::UnallocatedOverhead { .. }, _) => 2,
        (AiBillingCharge::Metered(charge), project) => match charge.kind {
            AiMeteredChargeKind::Project | AiMeteredChargeKind::Repository => {
                u8::from(project.is_none())
            }
            AiMeteredChargeKind::Untracked => 3,
            AiMeteredChargeKind::Personal => 4,
            AiMeteredChargeKind::Account(_) => 5,
        },
        (_, None) => 1,
        (_, Some(_)) => 0,
    };
    let developer_of = |subject: &AiBillingSubject| -> String {
        subject
            .user_id()
            .and_then(|user_id| names.get(&user_id.as_i32()).copied())
            .unwrap_or("")
            .to_string()
    };
    let developer = |part: &AiBillingLinePart| developer_of(&part.line.subject);
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by(|a, b| {
        let (a, b) = (&parts[*a], &parts[*b]);
        project_rank(a)
            .cmp(&project_rank(b))
            .then_with(|| {
                let name = |part: &AiBillingLinePart| {
                    part.line
                        .project
                        .as_ref()
                        .map(|project| (project.name.clone(), project.id.to_string()))
                };
                name(a).cmp(&name(b))
            })
            .then_with(|| a.unassigned.cmp(&b.unassigned))
            .then_with(|| developer(a).cmp(&developer(b)))
            .then_with(|| a.line.subject.order_key().cmp(&b.line.subject.order_key()))
            .then_with(|| a.line.provider.as_str().cmp(b.line.provider.as_str()))
            .then_with(|| {
                let mode = |part: &AiBillingLinePart| match part.line.charge {
                    AiBillingCharge::Subscription { .. }
                    | AiBillingCharge::UnallocatedOverhead { .. } => 0,
                    AiBillingCharge::Api => 1,
                    AiBillingCharge::Metered(_) => 2,
                };
                mode(a).cmp(&mode(b))
            })
            .then_with(|| {
                let account = |part: &AiBillingLinePart| {
                    metered(part)
                        .and_then(|charge| charge.account.as_ref())
                        .map(|account| (account.login.as_str().to_lowercase(), account.account_id))
                };
                account(a).cmp(&account(b))
            })
            .then_with(|| {
                let sku =
                    |part: &AiBillingLinePart| metered(part).and_then(|charge| charge.sku.clone());
                sku(a).cmp(&sku(b))
            })
    });

    let snapshot_id = close
        .map(|close| close.as_i64().to_string())
        .unwrap_or_default();
    let mut rows = Vec::with_capacity(parts.len() + bill.adjustments.len());
    for part in parts {
        let line = part.line;
        let converted = &part.converted;
        let subscription = match &line.charge {
            AiBillingCharge::Api | AiBillingCharge::Metered(_) => None,
            AiBillingCharge::Subscription {
                subscription_id, ..
            }
            | AiBillingCharge::UnallocatedOverhead {
                subscription_id, ..
            } => subscriptions.get(&subscription_id.as_i32()).copied(),
        };
        let payer = payer_name(line.provider);
        let (project_id, project) = match (&line.charge, &line.project) {
            (AiBillingCharge::UnallocatedOverhead { .. }, _) => {
                (String::new(), UNALLOCATED_OVERHEAD.to_string())
            }
            (AiBillingCharge::Metered(charge), project) => {
                metered_project(charge.kind, project.as_ref(), payer)
            }
            (_, Some(project)) => (project.id.to_string(), project.name.clone()),
            (_, None) => (String::new(), UNASSIGNED.to_string()),
        };
        let usage = &part.usage;
        let unknown_cost = usage.priced_cost == AiUsdNanos::ZERO && usage.unpriced_records > 0;
        let (billing_mode, source) = match &line.charge {
            AiBillingCharge::Api => ("api", "estimate"),
            AiBillingCharge::Subscription { .. } | AiBillingCharge::UnallocatedOverhead { .. } => {
                ("subscription", "subscription")
            }
            AiBillingCharge::Metered(_) => ("metered", metered_source(line.provider)),
        };
        let billable = &part.billable;
        let (billable_amount, billable_currency) = match (&line.charge, billable) {
            (_, Some(amount)) => (amount.to_decimal(), amount.currency.as_str().to_string()),
            (AiBillingCharge::Api, None) => (String::new(), "USD".to_string()),
            (_, None) => (String::new(), String::new()),
        };
        let basis = subscription.and_then(|month| month.basis);
        let rate = converted.rate();
        let charge = metered(part);

        rows.push(vec![
            bill.month.to_string(),
            project_id,
            project,
            developer(part),
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
                (AiBillingCharge::Api | AiBillingCharge::Metered(_), _) => "",
                (_, Some(AiAllocationBasis::ApiCost)) => "api_cost",
                (_, Some(AiAllocationBasis::Tokens)) => "tokens",
                (_, Some(AiAllocationBasis::Records)) => "records",
                (_, None) => "none",
            }
            .to_string(),
            warning(part, basis, &overview.converted.pending_rates),
            unassigned_text(part.unassigned).to_string(),
            source.to_string(),
            charge
                .map(|charge| format!("{}.00", charge.credits()))
                .unwrap_or_default(),
            charge
                .and_then(|charge| charge.account.as_ref())
                .map(|account| account.login.as_str().to_string())
                .unwrap_or_default(),
            repository_name(&line.subject),
            charge
                .and_then(|charge| charge.sku.clone())
                .unwrap_or_default(),
            if charge.is_some() {
                "UTC".to_string()
            } else {
                time_zone.to_string()
            },
            String::new(),
            snapshot_id.clone(),
            charge
                .map(|charge| (!charge.provisional.is_empty()).to_string())
                .unwrap_or_default(),
        ]);
    }
    for line in &bill.adjustments {
        rows.push(adjustment_row(
            bill,
            line,
            developer_of(&line.subject),
            time_zone,
        ));
    }
    order.extend(parts.len()..rows.len());

    Ok(CsvTable {
        header: csv_header().collect(),
        rows,
        order,
    })
}

fn unassigned_text(kind: Option<AiUnassignedKind>) -> &'static str {
    match kind {
        Some(AiUnassignedKind::Unmapped) => "unmapped",
        Some(AiUnassignedKind::UnmappedRepository) => "unmapped_repository",
        Some(AiUnassignedKind::Unattributed) => "unattributed",
        None => "",
    }
}

fn repository_name(subject: &AiBillingSubject) -> String {
    match subject {
        AiBillingSubject::Repository { name, .. } => name.clone(),
        _ => String::new(),
    }
}

/// An account subject's login, the `github_login` of its adjustment rows: a
/// user's adjustment sums their accounts, so it names none.
fn account_login(subject: &AiBillingSubject) -> String {
    match subject {
        AiBillingSubject::ProviderAccount { login, .. } => login.as_str().to_string(),
        _ => String::new(),
    }
}

/// A metered line's project columns: its project, Unassigned, or one of
/// Spinit's labels.
fn metered_project(
    kind: AiMeteredChargeKind,
    project: Option<&crate::domain::models::AiMappedProject>,
    payer: &str,
) -> (String, String) {
    match (kind, project) {
        (AiMeteredChargeKind::Project | AiMeteredChargeKind::Repository, Some(project)) => {
            (project.id.to_string(), project.name.clone())
        }
        (AiMeteredChargeKind::Project | AiMeteredChargeKind::Repository, None) => {
            (String::new(), UNASSIGNED.to_string())
        }
        (AiMeteredChargeKind::Untracked, _) => (String::new(), format!("Untracked {payer}")),
        (AiMeteredChargeKind::Personal, _) => {
            (String::new(), format!("Personal / internal {payer}"))
        }
        (AiMeteredChargeKind::Account(AiMeteredAccountReason::Unlinked), _) => {
            (String::new(), "Unlinked account".to_string())
        }
        (AiMeteredChargeKind::Account(AiMeteredAccountReason::External), _) => {
            (String::new(), "External account".to_string())
        }
        (AiMeteredChargeKind::Account(AiMeteredAccountReason::Unknown), _) => {
            (String::new(), "Unknown account".to_string())
        }
    }
}

/// An adjustment line's row: signed amounts at the rate of the month it
/// corrects, usage columns zero.
fn adjustment_row(
    bill: &AiMonthBill,
    line: &AiAdjustmentLine,
    developer: String,
    time_zone: &str,
) -> Vec<String> {
    let payer = payer_name(line.provider);
    let (project_id, project) = match line.key.kind {
        AiAdjustedKind::Overhead => (String::new(), UNALLOCATED_OVERHEAD.to_string()),
        AiAdjustedKind::Metered(kind) => metered_project(kind, line.project.as_ref(), payer),
        AiAdjustedKind::Api | AiAdjustedKind::Subscription => match &line.project {
            Some(project) => (project.id.to_string(), project.name.clone()),
            None => (String::new(), UNASSIGNED.to_string()),
        },
    };
    let source = match line.key.kind {
        AiAdjustedKind::Api => "estimate",
        AiAdjustedKind::Subscription | AiAdjustedKind::Overhead => "subscription",
        AiAdjustedKind::Metered(_) => metered_source(line.provider),
    };
    let rate = line.converted.rate();
    let adjusted = line.adjusts.month;
    let snapshot = line.adjusts.close_id.as_i64();
    let warning = adjustment_warning(
        adjusted,
        snapshot,
        &line.converted,
        line.sek_only(),
        line.rate_changed,
    );
    vec![
        bill.month.to_string(),
        project_id,
        project,
        developer,
        line.provider.as_str().to_string(),
        "adjustment".to_string(),
        line.plan.clone().unwrap_or_default(),
        line.amount.to_decimal(),
        line.amount.currency.as_str().to_string(),
        line.converted
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
        AiUsdNanos::ZERO.to_decimal(4),
        "0".to_string(),
        "0".to_string(),
        "0".to_string(),
        "0".to_string(),
        "0".to_string(),
        "0".to_string(),
        String::new(),
        warning,
        unassigned_text(line.unassigned).to_string(),
        source.to_string(),
        line.credits
            .map(|credits| format!("{credits}.00"))
            .unwrap_or_default(),
        account_login(&line.subject),
        repository_name(&line.subject),
        String::new(),
        if line.key.kind.is_metered() {
            "UTC".to_string()
        } else {
            time_zone.to_string()
        },
        adjusted.to_string(),
        snapshot.to_string(),
        String::new(),
    ]
}

/// An adjustment row's warning: the month it corrects and its close, and
/// what changed in the billing currency. SEK alone moves when the corrected
/// month's rate changed since the key was last invoiced (`rate_changed`), or
/// when its SEK amounts round differently at the same rate.
fn adjustment_warning(
    adjusted: AiBillingMonth,
    snapshot: i64,
    converted: &AiConvertedAmount,
    sek_only: bool,
    rate_changed: bool,
) -> String {
    let mut warning =
        format!("adjusts {adjusted}: its bill changed after it closed (snapshot {snapshot})");
    match converted {
        AiConvertedAmount::MissingRate { currency } => warning.push_str(&format!(
            "; no {} exchange rate for {adjusted}",
            currency.as_str()
        )),
        _ if sek_only && rate_changed => {
            warning.push_str(&format!("; SEK only: {adjusted}'s exchange rate changed"))
        }
        _ if sek_only => {
            warning.push_str("; SEK only: its SEK amounts round differently at the same rate")
        }
        _ if rate_changed => warning.push_str(&format!("; {adjusted}'s exchange rate changed too")),
        _ => {}
    }
    warning
}

/// A metered line's provisional reason, as the warning says it.
fn provisional_reason(reason: AiMeteredProvisional) -> &'static str {
    match reason {
        AiMeteredProvisional::MonthInProgress => "the UTC month is in progress",
        AiMeteredProvisional::ImportIncomplete => {
            "the usage import is missing or incomplete for some days"
        }
        AiMeteredProvisional::NotSettled => "some days may still change",
        AiMeteredProvisional::OldPricingBasis => "tracked usage on an old pricing basis",
        AiMeteredProvisional::ProvisionalModel => "a model with provisional rates",
        AiMeteredProvisional::Unrecognized => "an unrecognized model or credits row",
    }
}

/// What needs a look on a metered line.
fn metered_warnings(charge: &AiMeteredCharge, unpriced: i64) -> Vec<String> {
    let mut warnings = Vec::new();
    if !charge.provisional.is_empty() {
        let reasons: Vec<&str> = charge
            .provisional
            .iter()
            .map(|reason| provisional_reason(*reason))
            .collect();
        warnings.push(format!("provisional: {}", reasons.join(", ")));
    }
    if charge.low_ratio {
        warnings.push(
            "G/T below 0.85 on a settled day: GitHub billed far less than Toki tracked".to_string(),
        );
    }
    if unpriced > 0 {
        warnings.push(format!(
            "{unpriced} unpriced records: they carry no weight in the split"
        ));
    }
    if charge.kind == AiMeteredChargeKind::Account(AiMeteredAccountReason::Unknown) {
        warnings.push("code review is billed by repository and left out".to_string());
    }
    warnings
}

/// Why a row's figures need a second look, if they do.
fn warning(
    part: &AiBillingLinePart,
    basis: Option<AiAllocationBasis>,
    pending: &[AiCurrency],
) -> String {
    let line = part.line;
    let converted = &part.converted;
    let unpriced = part.usage.unpriced_records;
    let mut warnings = Vec::new();
    match (&line.charge, basis) {
        (AiBillingCharge::Metered(charge), _) => {
            warnings.extend(metered_warnings(charge, unpriced));
            if part.unassigned == Some(AiUnassignedKind::UnmappedRepository) {
                warnings.push(
                    "repository without a project: map it under Project mappings > GitHub repositories"
                        .to_string(),
                );
            }
        }
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
/// a program that splits the value stays on text. A value that is wholly a
/// number (`-?\d+(\.\d+)?`), such as an adjustment's `-1.50`, is never a
/// formula and stays a number; `-1+2` or `--1` are guarded.
pub(super) fn csv_cell(value: &str) -> String {
    if is_number(value) {
        return format!("\"{value}\"");
    }
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

/// Whether `value` is a plain decimal number with an optional minus sign:
/// `-?\d+(\.\d+)?`.
fn is_number(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = match digits.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (digits, None),
    };
    let all_digits =
        |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    all_digits(whole) && fraction.is_none_or(all_digits)
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
            // Numbers stay numbers, negative ones (adjustments) included.
            ("-5", r#""-5""#),
            ("-1.50", r#""-1.50""#),
            ("-0.05", r#""-0.05""#),
            ("-1+2", r#""'-1+2""#),
            ("-5x", r#""'-5x""#),
            ("--1", r#""'--1""#),
            ("-", r#""'-""#),
            ("-.5", r#""'-.5""#),
            ("-1.", r#""'-1.""#),
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

    #[test]
    fn an_adjustment_names_a_rate_change_only_when_the_rate_changed() {
        let june = AiBillingMonth::parse("2026-06").unwrap();
        let sek = AiConvertedAmount::Converted {
            amount: AiFeeAmount {
                hundredths: -1,
                currency: AiCurrency::billing(),
            },
            rate: None,
        };
        let base = "adjusts 2026-06: its bill changed after it closed (snapshot 7)";
        for (sek_only, rate_changed, rest) in [
            (true, true, "; SEK only: 2026-06's exchange rate changed"),
            (
                true,
                false,
                "; SEK only: its SEK amounts round differently at the same rate",
            ),
            (false, true, "; 2026-06's exchange rate changed too"),
            (false, false, ""),
        ] {
            assert_eq!(
                adjustment_warning(june, 7, &sek, sek_only, rate_changed),
                format!("{base}{rest}"),
                "{sek_only} {rate_changed}"
            );
        }
        let missing = AiConvertedAmount::MissingRate {
            currency: AiCurrency::parse("EUR").unwrap(),
        };
        assert_eq!(
            adjustment_warning(june, 7, &missing, false, false),
            format!("{base}; no EUR exchange rate for 2026-06")
        );
    }
}
