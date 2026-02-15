pub(crate) fn normalize_email(email: &str) -> Option<String> {
    let normalized = email.trim().to_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_email;

    #[test]
    fn normalize_email_trims_and_lowercases() {
        assert_eq!(
            normalize_email("  USER@Example.com  "),
            Some("user@example.com".to_string())
        );
        assert_eq!(normalize_email("   "), None);
    }
}
