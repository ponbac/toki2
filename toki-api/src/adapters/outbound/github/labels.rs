//! GitHub's billing strings, mapped to Toki's terms. The only place that
//! knows them.
//!
//! - Model labels: "Auto: X" counts as X (auto-routed, already discounted).
//!   An override table holds the special labels; any other label is lower
//!   cased with each run of whitespace replaced by `-`, which gives the model
//!   names Toki's usage buckets carry ("GPT-6.1 Sol" → `gpt-6.1-sol`).
//! - Credits are worth $0.01 only on rows priced at 0.01 per AI credit; any
//!   other unit or price is unrecognized, kept and flagged.

use crate::domain::models::{
    AiCreditKind, AiDecimal, AiMeteredModel, AiOrgChargeKind, AiSummaryKind,
};

/// GitHub's special labels: (label, model, kind), matched trimmed and
/// case-insensitively. Code review credits are billed per repository too.
const LABEL_OVERRIDES: &[(&str, &str, AiCreditKind)] = &[(
    "Code Review model",
    "code-review",
    AiCreditKind::RepositoryBilled,
)];
/// The prefix of automatically routed usage, matched case-insensitively.
const AUTO_PREFIX: &str = "auto:";
/// The SKU of credits Toki tracks.
const CREDITS_SKU: &str = "Copilot AI Credits";
/// The SKU of the cloud agent's credits, which Toki cannot track.
const CLOUD_AGENT_SKU: &str = "Copilot Cloud Agent";
/// The SKU of seat fees in the usage rows.
const SEATS_SKU: &str = "Copilot Business";

/// The model a label names, and the kind an override gives it. An empty
/// label is model "".
pub(super) fn model(label: &str) -> (AiMeteredModel, Option<AiCreditKind>) {
    let trimmed = label.trim();
    let (rest, auto_routed) = match trimmed.get(..AUTO_PREFIX.len()) {
        Some(prefix) if prefix.eq_ignore_ascii_case(AUTO_PREFIX) => {
            (trimmed[AUTO_PREFIX.len()..].trim(), true)
        }
        _ => (trimmed, false),
    };
    if let Some((_, model, kind)) = LABEL_OVERRIDES
        .iter()
        .find(|(override_label, _, _)| override_label.eq_ignore_ascii_case(rest))
    {
        return (
            AiMeteredModel {
                label: label.to_string(),
                model: (*model).to_string(),
                auto_routed,
            },
            Some(*kind),
        );
    }
    let model = rest
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    (
        AiMeteredModel {
            label: label.to_string(),
            model,
            auto_routed,
        },
        None,
    )
}

/// Whether a row is priced at 0.01 per AI credit: its unit, letters only and
/// case-insensitively, is "aicredits" (`ai-credits`, `AICredits`).
fn is_credit_unit(unit: &str, unit_price: &AiDecimal) -> bool {
    let letters: String = unit
        .chars()
        .filter(char::is_ascii_alphabetic)
        .collect::<String>()
        .to_ascii_lowercase();
    letters == "aicredits" && AiDecimal::parse("0.01").is_some_and(|cent| *unit_price == cent)
}

/// How a credits row is billed. Another unit or unit price is unrecognized;
/// an override wins; then by SKU, an empty model being unrecognized.
pub(super) fn credit_kind(
    sku: &str,
    unit: &str,
    unit_price: &AiDecimal,
    label_kind: Option<AiCreditKind>,
    model: &str,
) -> AiCreditKind {
    if !is_credit_unit(unit, unit_price) {
        return AiCreditKind::Unrecognized;
    }
    if let Some(kind) = label_kind {
        return kind;
    }
    if model.is_empty() {
        return AiCreditKind::Unrecognized;
    }
    match sku.trim() {
        sku if sku.eq_ignore_ascii_case(CREDITS_SKU) => AiCreditKind::Trackable,
        sku if sku.eq_ignore_ascii_case(CLOUD_AGENT_SKU) => AiCreditKind::Untrackable,
        _ => AiCreditKind::Unrecognized,
    }
}

/// What an organization usage row is: Copilot credits at 0.01 per AI credit
/// with a repository are that repository's, and without one the IDE
/// aggregate; the cloud agent's are untrackable; Actions is compute; Copilot
/// Business is seats; anything else is unrecognized.
pub(super) fn org_charge_kind(
    product: &str,
    sku: &str,
    unit: &str,
    unit_price: &AiDecimal,
    repository: &str,
) -> AiOrgChargeKind {
    let product = product.trim();
    let sku = sku.trim();
    let copilot = product.eq_ignore_ascii_case("copilot");
    if copilot && sku.eq_ignore_ascii_case(CREDITS_SKU) && is_credit_unit(unit, unit_price) {
        if repository.is_empty() {
            AiOrgChargeKind::AccountCreditsAggregate
        } else {
            AiOrgChargeKind::RepositoryCredits
        }
    } else if copilot
        && sku.eq_ignore_ascii_case(CLOUD_AGENT_SKU)
        && is_credit_unit(unit, unit_price)
    {
        AiOrgChargeKind::UntrackableCredits
    } else if product.eq_ignore_ascii_case("actions") {
        AiOrgChargeKind::Compute
    } else if copilot && sku.eq_ignore_ascii_case(SEATS_SKU) {
        AiOrgChargeKind::Seats
    } else {
        AiOrgChargeKind::Unrecognized
    }
}

/// What a month summary row is, by its SKU.
pub(super) fn summary_kind(sku: &str) -> AiSummaryKind {
    match sku.trim() {
        "copilot_ai_unit" => AiSummaryKind::Credits,
        "coding_agent_ai_unit" => AiSummaryKind::UntrackableCredits,
        "copilot_for_business" => AiSummaryKind::Seats,
        _ => AiSummaryKind::Unrecognized,
    }
}
