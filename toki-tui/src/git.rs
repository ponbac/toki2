/// Conventional commit prefixes that get "prefix: rest" treatment.
const CONVENTIONAL_PREFIXES: &[&str] = &[
    "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert",
    "feature",
];

fn is_conventional(prefix: &str) -> bool {
    CONVENTIONAL_PREFIXES.contains(&prefix)
}

/// Extract a "standalone" number from `s`.
///
/// Standalone means the digit run is:
/// - preceded by `-`, `_`, OR is at the start of the string, AND
/// - followed by `-`, `_`, OR is at the end of the string.
///
/// Returns `(Some(number_str), rest_without_number_and_separator)` if found,
/// otherwise `(None, s)`.
fn extract_standalone_number(s: &str) -> (Option<String>, String) {
    let bytes = s.as_bytes();
    let len = s.len();

    let mut i = 0;
    while i < len {
        // Check if bytes[i] starts a digit run
        if bytes[i].is_ascii_digit() {
            // Find start and end of digit run
            let digit_start = i;
            let mut j = i;
            while j < len && bytes[j].is_ascii_digit() {
                j += 1;
            }
            let digit_end = j; // exclusive

            // Check preceding condition
            let preceded_ok = digit_start == 0
                || bytes[digit_start - 1] == b'-'
                || bytes[digit_start - 1] == b'_';

            // Check following condition
            let followed_ok =
                digit_end == len || bytes[digit_end] == b'-' || bytes[digit_end] == b'_';

            if preceded_ok && followed_ok {
                let number = s[digit_start..digit_end].to_string();

                // Build rest: remove the digit run plus any immediately adjacent separator
                // We remove: the separator before (if any) OR the separator after (if any),
                // keeping remaining text intact.
                let rest = if digit_start > 0
                    && (bytes[digit_start - 1] == b'-' || bytes[digit_start - 1] == b'_')
                {
                    // Remove separator-before + digits (+ separator-after if present)
                    let after_digits = if digit_end < len
                        && (bytes[digit_end] == b'-' || bytes[digit_end] == b'_')
                    {
                        digit_end + 1
                    } else {
                        digit_end
                    };
                    format!("{}{}", &s[..digit_start - 1], &s[after_digits..])
                } else {
                    // digit is at start of string; remove digits + trailing separator if present
                    let after_digits = if digit_end < len
                        && (bytes[digit_end] == b'-' || bytes[digit_end] == b'_')
                    {
                        digit_end + 1
                    } else {
                        digit_end
                    };
                    s[after_digits..].to_string()
                };

                return (Some(number), rest);
            }

            // Advance past the digit run
            i = j;
        } else {
            i += 1;
        }
    }

    (None, s.to_string())
}

/// Humanize a string: replace `-` and `_` with spaces, collapse whitespace, trim.
fn humanize(s: &str) -> String {
    let replaced = s.replace(['-', '_'], " ");
    // Collapse multiple spaces
    let mut result = String::new();
    let mut last_was_space = false;
    for ch in replaced.chars() {
        if ch == ' ' {
            if !last_was_space {
                result.push(' ');
            }
            last_was_space = true;
        } else {
            result.push(ch);
            last_was_space = false;
        }
    }
    result.trim().to_string()
}

/// Parse a git branch name into a human-readable description.
///
/// Rules (applied in order):
/// - Has number + conventional prefix → `"#NUM - prefix: humanized_rest"`
/// - Has number + no slash prefix     → `"#NUM - humanized_rest"`
/// - Has number + non-conventional slash prefix → `"#NUM - {default_prefix}: rest_after_slash"` (original)
/// - No number + conventional prefix  → `"prefix: humanized_rest"`
/// - No number + non-conventional slash prefix → `"{default_prefix}: branch"` (full original)
/// - No number + no slash              → `"{default_prefix}: branch"` (full original)
pub fn parse_branch(branch: &str, default_prefix: &str) -> String {
    // Step 1: Split on first `/` to get optional slash_prefix and rest.
    let (slash_prefix, rest) = if let Some(pos) = branch.find('/') {
        (Some(&branch[..pos]), &branch[pos + 1..])
    } else {
        (None, branch)
    };

    // Step 2: Extract standalone number from `rest`.
    let (number, rest_without_number) = extract_standalone_number(rest);

    // Step 3: Humanize the remaining rest.
    let humanized = humanize(&rest_without_number);

    // Step 4: Apply rules in order.
    match (number, slash_prefix) {
        (Some(num), Some(prefix)) if is_conventional(prefix) => {
            // Has number + conventional prefix
            format!("#{} - {}: {}", num, prefix, humanized)
        }
        (Some(num), None) => {
            // Has number + no slash prefix
            format!("#{} - {}", num, humanized)
        }
        (Some(num), Some(_prefix)) => {
            // Has number + non-conventional prefix
            // Use original rest_after_slash (not humanized)
            format!("#{} - {}: {}", num, default_prefix, rest)
        }
        (None, Some(prefix)) if is_conventional(prefix) => {
            // No number + conventional prefix
            format!("{}: {}", prefix, humanized)
        }
        _ => {
            // No number + non-conventional prefix, or no slash at all
            format!("{}: {}", default_prefix, branch)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_with_number_and_dashes() {
        assert_eq!(
            parse_branch("fix/8322-styling-adjustments", "Utveckling"),
            "#8322 - fix: styling adjustments"
        );
    }

    #[test]
    fn test_number_only_no_prefix() {
        assert_eq!(
            parse_branch("8322-mybranch", "Utveckling"),
            "#8322 - mybranch"
        );
    }

    #[test]
    fn test_feature_with_number_and_underscore() {
        assert_eq!(
            parse_branch("feature/8322_tests", "Utveckling"),
            "#8322 - feature: tests"
        );
    }

    #[test]
    fn test_main_no_slash_no_number() {
        assert_eq!(parse_branch("main", "Utveckling"), "Utveckling: main");
    }

    #[test]
    fn test_non_conventional_prefix_no_number() {
        assert_eq!(
            parse_branch("branding/testbageriet", "Utveckling"),
            "Utveckling: branding/testbageriet"
        );
    }

    #[test]
    fn test_conventional_prefix_no_number_embedded_digit_not_standalone() {
        // `2` in `feature2` is NOT standalone (no separator before it), so no number extracted.
        // `test` IS conventional, so result is "test: feature2".
        assert_eq!(
            parse_branch("test/feature2", "Utveckling"),
            "test: feature2"
        );
    }

    #[test]
    fn test_custom_default_prefix() {
        assert_eq!(parse_branch("main", "Development"), "Development: main");
        assert_eq!(
            parse_branch("branding/testbageriet", "Development"),
            "Development: branding/testbageriet"
        );
    }
}
