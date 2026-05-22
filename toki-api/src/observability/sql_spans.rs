use std::{borrow::Cow, fmt, time::Duration};

use opentelemetry::{KeyValue, Value as OtelValue};
use opentelemetry_sdk::{
    error::OTelSdkResult,
    trace::{SpanData, SpanExporter},
    Resource,
};

pub(super) struct SqlSpanNamingExporter<E> {
    inner: E,
}

impl<E> SqlSpanNamingExporter<E> {
    pub(super) fn new(inner: E) -> Self {
        Self { inner }
    }
}

impl<E> fmt::Debug for SqlSpanNamingExporter<E>
where
    E: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqlSpanNamingExporter")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<E> SpanExporter for SqlSpanNamingExporter<E>
where
    E: SpanExporter,
{
    fn export(
        &self,
        mut batch: Vec<SpanData>,
    ) -> impl std::future::Future<Output = OTelSdkResult> + Send {
        for span in &mut batch {
            rename_sqlx_span(span);
        }

        self.inner.export(batch)
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.inner.set_resource(resource);
    }
}

pub(super) fn query_summary(sql: &str) -> String {
    let summary = sql.split_whitespace().take(8).collect::<Vec<_>>().join(" ");

    if summary.is_empty() {
        "SQL".to_string()
    } else {
        summary
    }
}

fn rename_sqlx_span(span: &mut SpanData) {
    if !span.name.as_ref().starts_with("sqlx.") {
        return;
    }

    let Some(statement) = span_string_attribute(span, "db.query.text") else {
        return;
    };
    let Some(name) = db_query_name(&statement) else {
        return;
    };

    span.name = Cow::Owned(name.summary.clone());
    upsert_span_attribute(
        &mut span.attributes,
        "db.operation.name",
        name.operation.clone(),
    );
    upsert_span_attribute(
        &mut span.attributes,
        "db.query.summary",
        name.summary.clone(),
    );

    if let Some(collection) = name.collection {
        upsert_span_attribute(
            &mut span.attributes,
            "db.collection.name",
            collection.clone(),
        );
        upsert_span_attribute(&mut span.attributes, "db.sql.table", collection);
    }
}

fn span_string_attribute(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|attribute| attribute.key.as_str() == key)
        .map(|attribute| attribute.value.as_str().into_owned())
}

fn upsert_span_attribute(attributes: &mut Vec<KeyValue>, key: &'static str, value: String) {
    if let Some(attribute) = attributes
        .iter_mut()
        .find(|attribute| attribute.key.as_str() == key)
    {
        attribute.value = OtelValue::from(value);
    } else {
        attributes.push(KeyValue::new(key, value));
    }
}

#[derive(Debug, PartialEq, Eq)]
struct DbQueryName {
    operation: String,
    collection: Option<String>,
    summary: String,
}

fn db_query_name(sql: &str) -> Option<DbQueryName> {
    let tokens = sql_tokens(sql);
    let operation_index = tokens.iter().position(|token| {
        matches!(
            token.upper.as_str(),
            "SELECT" | "INSERT" | "UPDATE" | "DELETE" | "WITH"
        )
    })?;
    let operation = tokens[operation_index].upper.clone();

    let collection = match operation.as_str() {
        "SELECT" => token_after_keyword(&tokens, operation_index, "FROM"),
        "INSERT" => token_after_keyword(&tokens, operation_index, "INTO"),
        "UPDATE" => tokens
            .get(operation_index + 1)
            .map(|token| token.raw.clone()),
        "DELETE" => token_after_keyword(&tokens, operation_index, "FROM"),
        _ => None,
    }
    .and_then(|token| normalize_sql_identifier(&token));

    let summary = collection
        .as_ref()
        .map(|collection| format!("{operation} {collection}"))
        .unwrap_or_else(|| operation.clone());

    Some(DbQueryName {
        operation,
        collection,
        summary,
    })
}

fn token_after_keyword(tokens: &[SqlToken], start: usize, keyword: &str) -> Option<String> {
    tokens
        .get(start..)?
        .windows(2)
        .find(|window| window[0].upper == keyword)
        .map(|window| window[1].raw.clone())
}

fn normalize_sql_identifier(identifier: &str) -> Option<String> {
    let identifier = identifier
        .trim_matches('"')
        .split('.')
        .next_back()?
        .trim_matches('"')
        .trim();

    if identifier.is_empty() {
        None
    } else {
        Some(identifier.to_string())
    }
}

#[derive(Debug, PartialEq, Eq)]
struct SqlToken {
    raw: String,
    upper: String,
}

fn sql_tokens(sql: &str) -> Vec<SqlToken> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut chars = sql.chars().peekable();

    while let Some(character) = chars.next() {
        if character == '-' && chars.peek() == Some(&'-') {
            chars.next();
            flush_sql_token(&mut tokens, &mut token);
            for character in chars.by_ref() {
                if character == '\n' {
                    break;
                }
            }
            continue;
        }

        if character == '/' && chars.peek() == Some(&'*') {
            chars.next();
            flush_sql_token(&mut tokens, &mut token);
            let mut previous = '\0';
            for character in chars.by_ref() {
                if previous == '*' && character == '/' {
                    break;
                }
                previous = character;
            }
            continue;
        }

        if character == '\'' {
            flush_sql_token(&mut tokens, &mut token);
            while let Some(character) = chars.next() {
                if character == '\'' {
                    if chars.peek() == Some(&'\'') {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            continue;
        }

        if character.is_ascii_alphanumeric() || matches!(character, '_' | '.') {
            token.push(character);
        } else if !token.is_empty() {
            flush_sql_token(&mut tokens, &mut token);
        }
    }

    flush_sql_token(&mut tokens, &mut token);
    tokens
}

fn flush_sql_token(tokens: &mut Vec<SqlToken>, token: &mut String) {
    if !token.is_empty() {
        tokens.push(SqlToken {
            upper: token.to_ascii_uppercase(),
            raw: std::mem::take(token),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_summary_uses_compact_first_words() {
        let sql = r#"
            SELECT id, email, full_name
            FROM users
            WHERE id = $1
            "#;

        assert_eq!(
            query_summary(sql),
            "SELECT id, email, full_name FROM users WHERE id"
        );
    }

    #[test]
    fn query_name_uses_operation_and_table() {
        assert_eq!(
            db_query_name("SELECT id FROM users WHERE id = $1"),
            Some(DbQueryName {
                operation: "SELECT".to_string(),
                collection: Some("users".to_string()),
                summary: "SELECT users".to_string(),
            })
        );
        assert_eq!(
            db_query_name("INSERT INTO timer_history (user_id) VALUES ($1)"),
            Some(DbQueryName {
                operation: "INSERT".to_string(),
                collection: Some("timer_history".to_string()),
                summary: "INSERT timer_history".to_string(),
            })
        );
        assert_eq!(
            db_query_name("UPDATE public.notifications SET read = true"),
            Some(DbQueryName {
                operation: "UPDATE".to_string(),
                collection: Some("notifications".to_string()),
                summary: "UPDATE notifications".to_string(),
            })
        );
        assert_eq!(
            db_query_name("-- generated by sqlx\nDELETE FROM push_subscriptions WHERE id = $1"),
            Some(DbQueryName {
                operation: "DELETE".to_string(),
                collection: Some("push_subscriptions".to_string()),
                summary: "DELETE push_subscriptions".to_string(),
            })
        );
    }

    #[test]
    fn query_name_ignores_sql_comments() {
        assert_eq!(
            db_query_name("SELECT 1 /* FROM ignored */ FROM users"),
            Some(DbQueryName {
                operation: "SELECT".to_string(),
                collection: Some("users".to_string()),
                summary: "SELECT users".to_string(),
            })
        );
    }
}
