//! Suggested targets for provider accounts: which Toki user an account
//! probably is, from its public profile and its login.
//!
//! Names are folded before they are compared (`fold_name`): lower case, a few
//! letters spelled out (ø → o, æ → ae, ß → ss, …), and accents dropped. Rules
//! come in three families, evaluated in this order, and the first match in each
//! family counts as evidence:
//!
//! | Family | Evidence | Rule |
//! |---|---|---|
//! | email | `Email` | the public email is the user's (trimmed, any case) |
//! | name | `Name` | ≥ 2 name tokens, the user's in order or as the same multiset |
//! | name | `NameFirstLast` | ≥ 2 tokens, the user's first and last |
//! | name | `NameInitial` | ≥ 2 tokens, the user's first, then the last's initial |
//! | name | `NameFirstOnly` | 1 token, a first name no other user has |
//! | login | `LoginFullName` | the login's core is first+last, last+first or every token |
//! | login | `LoginInitialSurname` | ≥ 6 characters: the first's initial, then a prefix of the last |
//! | login | `LoginFirstSurnamePrefix` | the first, then at least 2 leading characters of the last |
//! | login | `LoginFirstOnly` | a first name no other user has |
//!
//! A login's core drops digits and the namespace's own words, such as
//! `spinit` for Spinit-AB (`login_core`).
//!
//! A candidate is **verified** only with email or full-name evidence, when it
//! is the account's only candidate with such evidence and no other account has
//! such evidence for the same user. Everything else is **probable**: login
//! evidence and usage activity never verify. Candidates are ranked by
//! confidence, their best evidence, and the user's activity at the payer.

use std::collections::HashMap;

use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

use super::{
    AiAccountDisposition, AiAccountPeriod, AiBillingDeveloper, AiProviderAccountId, AiUserActivity,
    UserId,
};

/// How sure a suggestion is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AiSuggestionConfidence {
    Verified,
    Probable,
}

/// Why a user was suggested; see the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AiSuggestionEvidence {
    Email,
    Name,
    NameFirstLast,
    NameInitial,
    NameFirstOnly,
    LoginFullName,
    LoginInitialSurname,
    LoginFirstSurnamePrefix,
    LoginFirstOnly,
}

impl AiSuggestionEvidence {
    /// Whether the evidence can verify a candidate.
    pub fn is_strong(self) -> bool {
        matches!(self, Self::Email | Self::Name)
    }

    /// The evidence's place in the ranking; lower is better.
    fn rank(self) -> u8 {
        match self {
            Self::Email => 0,
            Self::Name => 1,
            Self::NameFirstLast => 2,
            Self::LoginFullName => 3,
            Self::NameInitial => 4,
            Self::LoginInitialSurname => 5,
            Self::LoginFirstSurnamePrefix => 6,
            Self::NameFirstOnly => 7,
            Self::LoginFirstOnly => 8,
        }
    }
}

/// A user suggested for an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAccountSuggestion {
    pub user_id: UserId,
    pub confidence: AiSuggestionConfidence,
    /// In evaluation order: email, then name, then login.
    pub evidence: Vec<AiSuggestionEvidence>,
}

/// The most picks one "accept suggestions" request may carry.
pub const MAX_ACCEPTED_SUGGESTIONS: usize = 100;

/// Why an accepted suggestion was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiSuggestionSkip {
    /// The user is the account's top suggestion, but only a probable one.
    NotVerified,
    /// The account's top suggestion is another user now, or there is none.
    Changed,
    /// The account already has a disposition.
    AlreadyDisposed,
    /// The account's default period overlaps one of its dispositions.
    AccountOverlap,
    /// The user is linked to another account on some of those days.
    UserOverlap,
}

/// What accepting suggestions did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAcceptedSuggestions {
    pub created: Vec<AiAccountDisposition>,
    pub skipped: Vec<(AiProviderAccountId, AiSuggestionSkip)>,
}

/// What suggestions read about an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSuggestionAccount {
    pub account_id: AiProviderAccountId,
    pub login: String,
    pub name: Option<String>,
    pub email: Option<String>,
    /// The period a new disposition would get (`default_period`); an account
    /// without one gets no suggestions.
    pub default_period: Option<AiAccountPeriod>,
}

/// Lower case, with ø → o, æ → ae, ß → ss, ł → l, đ → d, þ → th and œ → oe,
/// and combining marks dropped after compatibility decomposition, so `Åsa
/// Ängström` folds to `asa angstrom`.
pub fn fold_name(raw: &str) -> String {
    raw.to_lowercase()
        .chars()
        .flat_map(|ch| {
            let spelled: &[char] = match ch {
                'ø' => &['o'],
                'æ' => &['a', 'e'],
                'ß' => &['s', 's'],
                'ł' => &['l'],
                'đ' => &['d'],
                'þ' => &['t', 'h'],
                'œ' => &['o', 'e'],
                _ => &[],
            };
            if spelled.is_empty() {
                vec![ch]
            } else {
                spelled.to_vec()
            }
        })
        .collect::<String>()
        .nfkd()
        .filter(|ch| !is_combining_mark(*ch))
        .collect()
}

/// The folded name's runs of `a`–`z` and `0`–`9`.
pub fn name_tokens(raw: &str) -> Vec<String> {
    fold_name(raw)
        .split(|ch: char| !(ch.is_ascii_lowercase() || ch.is_ascii_digit()))
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// The words of a namespace, such as an organization, that logins often carry
/// as an affix: its tokens of at least 3 characters (`Spinit-AB` → `spinit`).
pub fn namespace_noise(namespace: &str) -> Vec<String> {
    let mut noise: Vec<String> = name_tokens(namespace)
        .into_iter()
        .filter(|token| token.len() >= 3)
        .collect();
    noise.dedup();
    noise
}

/// A login without its digits and namespace words: the folded login's runs of
/// letters, without noise tokens, joined; then each noise word is stripped
/// once from the end and once from the start while more remains
/// (`hedy02spinit` → `hedy`).
pub fn login_core(login: &str, noise: &[String]) -> String {
    let mut core: String = fold_name(login)
        .split(|ch: char| !ch.is_ascii_lowercase())
        .filter(|token| !token.is_empty() && !noise.iter().any(|word| word == token))
        .collect();
    for word in noise {
        if core.len() > word.len() && core.ends_with(word.as_str()) {
            core.truncate(core.len() - word.len());
        }
        if core.len() > word.len() && core.starts_with(word.as_str()) {
            core.drain(..word.len());
        }
    }
    core
}

/// A user's folded name and email, ready to compare.
struct ComparedUser {
    user_id: UserId,
    tokens: Vec<String>,
    email: String,
}

impl ComparedUser {
    fn first(&self) -> &str {
        &self.tokens[0]
    }

    fn last(&self) -> &str {
        &self.tokens[self.tokens.len() - 1]
    }
}

/// An account's folded profile and login core, ready to compare.
struct ComparedAccount {
    name: Vec<String>,
    email: Option<String>,
    core: String,
}

/// A candidate user with the evidence for them.
type Candidate = (UserId, Vec<AiSuggestionEvidence>);

/// The evidence that `account` is `user`, in evaluation order.
fn evidence(
    account: &ComparedAccount,
    user: &ComparedUser,
    unique_first: impl Fn(&str) -> bool,
) -> Vec<AiSuggestionEvidence> {
    use AiSuggestionEvidence as E;

    let mut found = Vec::new();
    let (first, last) = (user.first(), user.last());

    if account
        .email
        .as_deref()
        .is_some_and(|email| !user.email.is_empty() && email == user.email)
    {
        found.push(E::Email);
    }

    let name = &account.name;
    let same_tokens = || {
        let (mut ours, mut theirs) = (name.clone(), user.tokens.clone());
        ours.sort();
        theirs.sort();
        ours == theirs
    };
    if name.len() >= 2 && (*name == user.tokens || same_tokens()) {
        found.push(E::Name);
    } else if name.len() >= 2 && name[0] == first && name[name.len() - 1] == last {
        found.push(E::NameFirstLast);
    } else if name.len() >= 2
        && name[0] == first
        && name[name.len() - 1].len() == 1
        && last.starts_with(name[name.len() - 1].as_str())
    {
        found.push(E::NameInitial);
    } else if name.len() == 1 && name[0] == first && unique_first(first) {
        found.push(E::NameFirstOnly);
    }

    let core = account.core.as_str();
    if !core.is_empty()
        && (core == format!("{first}{last}")
            || core == format!("{last}{first}")
            || core == user.tokens.concat())
    {
        found.push(E::LoginFullName);
    } else if core.len() >= 6
        && core.as_bytes()[0] == first.as_bytes()[0]
        && last.starts_with(&core[1..])
    {
        found.push(E::LoginInitialSurname);
    } else if core.len() > first.len()
        && core.starts_with(first)
        && common_prefix(&core[first.len()..], last) >= 2
    {
        found.push(E::LoginFirstSurnamePrefix);
    } else if core == first && unique_first(first) {
        found.push(E::LoginFirstOnly);
    }

    found
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(a, b)| a == b).count()
}

/// Suggestions for each account that has a default period, best first. Users
/// with a `User` disposition (of any account) that overlaps the account's
/// default period are not candidates. `activity` ranks equally good
/// candidates; `noise` is the namespace's words (`namespace_noise`).
pub fn suggest_targets(
    accounts: &[AiSuggestionAccount],
    users: &[AiBillingDeveloper],
    activity: &HashMap<UserId, AiUserActivity>,
    dispositions: &[AiAccountDisposition],
    noise: &[String],
) -> HashMap<AiProviderAccountId, Vec<AiAccountSuggestion>> {
    let users: Vec<ComparedUser> = users
        .iter()
        .map(|user| ComparedUser {
            user_id: user.user_id,
            tokens: name_tokens(&user.full_name),
            email: user.email.trim().to_lowercase(),
        })
        .filter(|user| !user.tokens.is_empty())
        .collect();
    let mut first_names: HashMap<&str, usize> = HashMap::new();
    for user in &users {
        *first_names.entry(user.first()).or_default() += 1;
    }
    let unique_first = |first: &str| first_names.get(first) == Some(&1);

    // Candidates with their evidence, per account.
    let candidates: Vec<(AiProviderAccountId, Vec<Candidate>)> = accounts
        .iter()
        .filter_map(|account| {
            let period = account.default_period?;
            let compared = ComparedAccount {
                name: account.name.as_deref().map(name_tokens).unwrap_or_default(),
                email: account
                    .email
                    .as_deref()
                    .map(|email| email.trim().to_lowercase())
                    .filter(|email| !email.is_empty()),
                core: login_core(&account.login, noise),
            };
            let linked = |user_id: UserId| {
                dispositions.iter().any(|disposition| {
                    disposition.terms.target.user_id() == Some(user_id)
                        && disposition.terms.period.overlaps(&period)
                })
            };
            let found = users
                .iter()
                .filter(|user| !linked(user.user_id))
                .filter_map(|user| {
                    let found = evidence(&compared, user, unique_first);
                    (!found.is_empty()).then_some((user.user_id, found))
                })
                .collect();
            Some((account.account_id, found))
        })
        .collect();

    // Strong evidence for one user from several accounts verifies none of them.
    let mut strong_by_user: HashMap<UserId, usize> = HashMap::new();
    for (_, found) in &candidates {
        for (user_id, evidence) in found {
            if evidence.iter().any(|evidence| evidence.is_strong()) {
                *strong_by_user.entry(*user_id).or_default() += 1;
            }
        }
    }

    candidates
        .into_iter()
        .map(|(account_id, found)| {
            let strong_here = found
                .iter()
                .filter(|(_, evidence)| evidence.iter().any(|evidence| evidence.is_strong()))
                .count();
            let mut suggestions: Vec<AiAccountSuggestion> = found
                .into_iter()
                .map(|(user_id, evidence)| {
                    let strong = evidence.iter().any(|evidence| evidence.is_strong());
                    let confidence =
                        if strong && strong_here == 1 && strong_by_user.get(&user_id) == Some(&1) {
                            AiSuggestionConfidence::Verified
                        } else {
                            AiSuggestionConfidence::Probable
                        };
                    AiAccountSuggestion {
                        user_id,
                        confidence,
                        evidence,
                    }
                })
                .collect();
            suggestions.sort_by_key(|suggestion| {
                let best = suggestion
                    .evidence
                    .iter()
                    .map(|evidence| evidence.rank())
                    .min()
                    .unwrap_or(u8::MAX);
                let (cost, records, days) = activity
                    .get(&suggestion.user_id)
                    .map(AiUserActivity::rank)
                    .unwrap_or_default();
                (
                    suggestion.confidence,
                    best,
                    std::cmp::Reverse(cost),
                    std::cmp::Reverse(records),
                    std::cmp::Reverse(days),
                    suggestion.user_id.as_i32(),
                )
            });
            (account_id, suggestions)
        })
        .collect()
}

#[cfg(test)]
mod tests;
