//! Query parser for extracting filters from natural language search queries.
//!
//! Transforms queries like "priority 1 bugs in Lerum" into structured filters.

use regex::Regex;
use std::sync::LazyLock;
use time::{Duration, OffsetDateTime};

use super::types::{ParsedQuery, SearchFilters, SearchSource};

/// Parse a natural language search query into structured filters and search text.
///
/// # Examples
///
/// ```
/// use toki_api::domain::search::{parse_query, SearchSource};
///
/// let parsed = parse_query("priority 1 bugs");
/// assert_eq!(parsed.filters.priority, Some(vec![1]));
/// assert_eq!(parsed.filters.item_type, Some(vec!["Bug".to_string()]));
///
/// let parsed = parse_query("authentication PRs");
/// assert_eq!(parsed.filters.source_type, Some(SearchSource::Pr));
/// assert_eq!(parsed.search_text, "authentication");
/// ```
pub fn parse_query(query: &str) -> ParsedQuery {
    let mut filters = SearchFilters::default();
    let mut remaining = query.to_string();

    // Extract source type
    remaining = extract_source_type(&remaining, &mut filters);

    // Extract priority
    remaining = extract_priority(&remaining, &mut filters);

    // Extract item type
    remaining = extract_item_type(&remaining, &mut filters);

    // Extract status
    remaining = extract_status(&remaining, &mut filters);

    // Extract date ranges
    remaining = extract_date_range(&remaining, &mut filters);

    // Extract draft filter
    remaining = extract_draft(&remaining, &mut filters);

    // Extract project (known projects)
    remaining = extract_project(&remaining, &mut filters);

    // Clean up remaining text
    let search_text = cleanup_search_text(&remaining);

    ParsedQuery {
        search_text,
        filters,
    }
}

// Regex patterns compiled once
static PRIORITY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bpriority\s*(\d+)\b").unwrap());
static PRIORITY_SHORT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bp([1-4])\b").unwrap());
static PR_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(PRs?|pull\s*requests?)\b").unwrap());
static WORK_ITEM_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(work\s*items?|WIs?)\b").unwrap());
static BUG_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(bugs?)\b").unwrap());
static TASK_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(tasks?)\b").unwrap());
static STORY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(user\s*stor(?:y|ies)|stor(?:y|ies))\b").unwrap());
static STATUS_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(active|completed|closed|resolved|abandoned|new|open)\b").unwrap()
});
static DATE_RANGE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(last|past)\s+(week|month|year|(\d+)\s*(days?|weeks?|months?))\b").unwrap()
});
static DRAFT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(drafts?|draft\s+PRs?)\b").unwrap());

fn extract_source_type(query: &str, filters: &mut SearchFilters) -> String {
    let mut result = query.to_string();

    if PR_PATTERN.is_match(query) {
        filters.source_type = Some(SearchSource::Pr);
        result = PR_PATTERN.replace_all(&result, "").to_string();
    } else if WORK_ITEM_PATTERN.is_match(query) {
        filters.source_type = Some(SearchSource::WorkItem);
        result = WORK_ITEM_PATTERN.replace_all(&result, "").to_string();
    }

    result
}

fn extract_priority(query: &str, filters: &mut SearchFilters) -> String {
    let mut result = query.to_string();
    let mut priorities = Vec::new();

    // Match "priority 1" or "priority 2"
    for cap in PRIORITY_PATTERN.captures_iter(query) {
        if let Ok(p) = cap[1].parse::<i32>() {
            if (1..=4).contains(&p) {
                priorities.push(p);
            }
        }
    }
    result = PRIORITY_PATTERN.replace_all(&result, "").to_string();

    // Match "p1" or "p2"
    for cap in PRIORITY_SHORT_PATTERN.captures_iter(query) {
        if let Ok(p) = cap[1].parse::<i32>() {
            priorities.push(p);
        }
    }
    result = PRIORITY_SHORT_PATTERN.replace_all(&result, "").to_string();

    if !priorities.is_empty() {
        priorities.sort();
        priorities.dedup();
        filters.priority = Some(priorities);
    }

    result
}

fn extract_item_type(query: &str, filters: &mut SearchFilters) -> String {
    let mut result = query.to_string();
    let mut types = Vec::new();

    if BUG_PATTERN.is_match(query) {
        types.push("Bug".to_string());
        result = BUG_PATTERN.replace_all(&result, "").to_string();
    }

    if TASK_PATTERN.is_match(query) {
        types.push("Task".to_string());
        result = TASK_PATTERN.replace_all(&result, "").to_string();
    }

    if STORY_PATTERN.is_match(query) {
        types.push("User Story".to_string());
        result = STORY_PATTERN.replace_all(&result, "").to_string();
    }

    if !types.is_empty() {
        filters.item_type = Some(types);
        // If we found work item types, assume work items unless explicitly PRs
        if filters.source_type.is_none() {
            filters.source_type = Some(SearchSource::WorkItem);
        }
    }

    result
}

fn extract_status(query: &str, filters: &mut SearchFilters) -> String {
    let mut result = query.to_string();
    let mut statuses = Vec::new();

    for cap in STATUS_PATTERN.captures_iter(query) {
        let status = normalize_status(&cap[1]);
        if !statuses.contains(&status) {
            statuses.push(status);
        }
    }
    result = STATUS_PATTERN.replace_all(&result, "").to_string();

    if !statuses.is_empty() {
        filters.status = Some(statuses);
    }

    result
}

fn normalize_status(status: &str) -> String {
    match status.to_lowercase().as_str() {
        "active" | "open" => "active".to_string(),
        "completed" | "closed" | "resolved" => "completed".to_string(),
        "abandoned" => "abandoned".to_string(),
        "new" => "new".to_string(),
        other => other.to_string(),
    }
}

fn extract_date_range(query: &str, filters: &mut SearchFilters) -> String {
    let mut result = query.to_string();

    if let Some(cap) = DATE_RANGE_PATTERN.captures(query) {
        let now = OffsetDateTime::now_utc();
        let duration = match cap.get(2).map(|m| m.as_str().to_lowercase()).as_deref() {
            Some("week") => Some(Duration::weeks(1)),
            Some("month") => Some(Duration::days(30)),
            Some("year") => Some(Duration::days(365)),
            _ => {
                // Parse "N days/weeks/months"
                if let (Some(num), Some(unit)) = (cap.get(3), cap.get(4)) {
                    if let Ok(n) = num.as_str().parse::<i64>() {
                        let unit_str = unit.as_str().to_lowercase();
                        if unit_str.starts_with("day") {
                            Some(Duration::days(n))
                        } else if unit_str.starts_with("week") {
                            Some(Duration::weeks(n))
                        } else if unit_str.starts_with("month") {
                            Some(Duration::days(n * 30))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        };

        if let Some(d) = duration {
            filters.updated_after = Some(now - d);
            result = DATE_RANGE_PATTERN.replace(&result, "").to_string();
        }
    }

    result
}

fn extract_draft(query: &str, filters: &mut SearchFilters) -> String {
    let mut result = query.to_string();

    if DRAFT_PATTERN.is_match(query) {
        filters.is_draft = Some(true);
        filters.source_type = Some(SearchSource::Pr);
        result = DRAFT_PATTERN.replace_all(&result, "").to_string();
    }

    result
}

static PROJECT_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (Regex::new(r"(?i)\b(in\s+)?lerum\b").unwrap(), "Lerums Djursjukhus"),
        (Regex::new(r"(?i)\b(in\s+)?evidensia\b").unwrap(), "Evidensia"),
    ]
});

fn extract_project(query: &str, filters: &mut SearchFilters) -> String {
    let mut result = query.to_string();

    for (re, full_name) in PROJECT_PATTERNS.iter() {
        if re.is_match(&result) {
            filters.project = Some(full_name.to_string());
            result = re.replace_all(&result, "").to_string();
            break;
        }
    }

    result
}

fn cleanup_search_text(text: &str) -> String {
    // Remove extra whitespace and common noise words
    let noise_words = ["in", "the", "for", "with", "from", "about"];

    let words: Vec<&str> = text
        .split_whitespace()
        .filter(|w| !noise_words.contains(&w.to_lowercase().as_str()))
        .collect();

    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_query() {
        let parsed = parse_query("");
        assert_eq!(parsed.search_text, "");
        assert!(parsed.filters.source_type.is_none());
    }

    #[test]
    fn parse_simple_text() {
        let parsed = parse_query("authentication");
        assert_eq!(parsed.search_text, "authentication");
        assert!(parsed.filters.source_type.is_none());
    }

    #[test]
    fn parse_pr_filter() {
        let parsed = parse_query("authentication PRs");
        assert_eq!(parsed.search_text, "authentication");
        assert_eq!(parsed.filters.source_type, Some(SearchSource::Pr));

        let parsed = parse_query("pull requests about auth");
        assert_eq!(parsed.filters.source_type, Some(SearchSource::Pr));
    }

    #[test]
    fn parse_work_item_filter() {
        let parsed = parse_query("authentication work items");
        assert_eq!(parsed.filters.source_type, Some(SearchSource::WorkItem));
    }

    #[test]
    fn parse_priority_filter() {
        let parsed = parse_query("priority 1 bugs");
        assert_eq!(parsed.filters.priority, Some(vec![1]));

        let parsed = parse_query("p2 issues");
        assert_eq!(parsed.filters.priority, Some(vec![2]));

        let parsed = parse_query("priority 1 and priority 2");
        assert_eq!(parsed.filters.priority, Some(vec![1, 2]));
    }

    #[test]
    fn parse_item_type_filter() {
        let parsed = parse_query("priority 1 bugs");
        assert_eq!(
            parsed.filters.item_type,
            Some(vec!["Bug".to_string()])
        );
        assert_eq!(parsed.filters.source_type, Some(SearchSource::WorkItem));

        let parsed = parse_query("tasks in Lerum");
        assert_eq!(
            parsed.filters.item_type,
            Some(vec!["Task".to_string()])
        );

        let parsed = parse_query("user stories");
        assert_eq!(
            parsed.filters.item_type,
            Some(vec!["User Story".to_string()])
        );
    }

    #[test]
    fn parse_status_filter() {
        let parsed = parse_query("active PRs");
        assert_eq!(parsed.filters.status, Some(vec!["active".to_string()]));

        let parsed = parse_query("closed bugs");
        assert_eq!(parsed.filters.status, Some(vec!["completed".to_string()]));

        // "resolved" normalizes to "completed"
        let parsed = parse_query("resolved work items");
        assert_eq!(parsed.filters.status, Some(vec!["completed".to_string()]));

        // "open" normalizes to "active"
        let parsed = parse_query("open PRs");
        assert_eq!(parsed.filters.status, Some(vec!["active".to_string()]));
    }

    #[test]
    fn parse_date_range_filter() {
        let parsed = parse_query("last week");
        assert!(parsed.filters.updated_after.is_some());
        let since = parsed.filters.updated_after.unwrap();
        let now = OffsetDateTime::now_utc();
        let diff = now - since;
        assert!(diff.whole_days() >= 6 && diff.whole_days() <= 8);

        let parsed = parse_query("past 30 days");
        assert!(parsed.filters.updated_after.is_some());
    }

    #[test]
    fn parse_draft_filter() {
        let parsed = parse_query("draft PRs");
        assert_eq!(parsed.filters.is_draft, Some(true));
        assert_eq!(parsed.filters.source_type, Some(SearchSource::Pr));
    }

    #[test]
    fn parse_project_filter() {
        let parsed = parse_query("bugs in Lerum");
        assert_eq!(
            parsed.filters.project,
            Some("Lerums Djursjukhus".to_string())
        );
    }

    #[test]
    fn parse_complex_query() {
        let parsed = parse_query("priority 1 bugs in Lerum closed last week");
        assert_eq!(parsed.filters.priority, Some(vec![1]));
        assert_eq!(
            parsed.filters.item_type,
            Some(vec!["Bug".to_string()])
        );
        assert_eq!(
            parsed.filters.project,
            Some("Lerums Djursjukhus".to_string())
        );
        assert_eq!(parsed.filters.status, Some(vec!["completed".to_string()]));
        assert!(parsed.filters.updated_after.is_some());
        assert_eq!(parsed.search_text, ""); // All tokens were filters
    }

    #[test]
    fn parse_preserves_search_text() {
        let parsed = parse_query("authentication PRs");
        assert_eq!(parsed.search_text, "authentication");

        let parsed = parse_query("fix login bug in authentication service");
        assert!(parsed.search_text.contains("fix"));
        assert!(parsed.search_text.contains("login"));
        assert!(parsed.search_text.contains("authentication"));
        assert!(parsed.search_text.contains("service"));
    }
}
