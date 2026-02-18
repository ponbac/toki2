use std::sync::Arc;
use std::{cmp::Ordering, collections::HashMap};

use async_trait::async_trait;

use crate::domain::{
    models::{
        synthetic_column_id_from_name, BoardColumn, BoardData, BoardState, Iteration, WorkItem,
        WorkItemImage,
    },
    ports::{inbound::WorkItemService, outbound::WorkItemProvider},
    WorkItemError,
};

/// Implementation of the WorkItemService inbound port.
///
/// This service orchestrates work item board operations by delegating to a
/// WorkItemProvider (outbound port) and adding business logic (sorting).
///
/// Single generic type param `P` — no local DB needed, unlike time tracking
/// which has both a client and a repository.
pub struct WorkItemServiceImpl<P: WorkItemProvider> {
    provider: Arc<P>,
}

impl<P: WorkItemProvider> WorkItemServiceImpl<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: WorkItemProvider> WorkItemService for WorkItemServiceImpl<P> {
    async fn get_iterations(&self) -> Result<Vec<Iteration>, WorkItemError> {
        self.provider.get_iterations().await
    }

    async fn get_board_data(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<BoardData, WorkItemError> {
        let ids = self
            .provider
            .query_work_item_ids(iteration_path, team)
            .await?;

        let mut items = if ids.is_empty() {
            vec![]
        } else {
            self.provider.get_work_items(&ids).await?
        };

        let mut columns = self.provider.get_board_columns(iteration_path, team).await;
        let assignments = self
            .provider
            .get_taskboard_column_assignments(iteration_path, team)
            .await;

        for item in &mut items {
            if let Some(assignment) = assignments.get(&item.id) {
                item.board_state = BoardState::from_taskboard_column(&assignment.column_name);
                item.board_column_id = assignment.column_id.clone();
                item.board_column_name = Some(assignment.column_name.clone());
            }
        }

        if columns.is_empty() {
            columns = fallback_columns();
            for item in &mut items {
                item.board_column_id = Some(item.board_state.fallback_column_id().to_string());
                item.board_column_name = Some(item.board_state.fallback_column_name().to_string());
            }
        } else {
            columns.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.name.cmp(&b.name)));
            columns.dedup_by(|a, b| a.id == b.id);
            ensure_items_have_matching_columns(&mut items, &mut columns);
            columns.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.name.cmp(&b.name)));
        }

        sort_items_by_column_and_priority(&mut items, &columns);

        Ok(BoardData { columns, items })
    }

    async fn format_work_item_for_llm(
        &self,
        work_item_id: &str,
    ) -> Result<(String, bool), WorkItemError> {
        self.provider.format_work_item_for_llm(work_item_id).await
    }

    async fn fetch_image(&self, image_url: &str) -> Result<WorkItemImage, WorkItemError> {
        self.provider.fetch_image(image_url).await
    }

    async fn move_work_item_to_column(
        &self,
        work_item_id: &str,
        target_column_name: &str,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<(), WorkItemError> {
        let work_item_id = work_item_id.trim();
        if work_item_id.is_empty() {
            return Err(WorkItemError::InvalidInput(
                "work_item_id cannot be empty".to_string(),
            ));
        }

        let target_column_name = target_column_name.trim();
        if target_column_name.is_empty() {
            return Err(WorkItemError::InvalidInput(
                "target_column_name cannot be empty".to_string(),
            ));
        }

        self.provider
            .move_work_item_to_column(work_item_id, target_column_name, iteration_path, team)
            .await
    }
}

fn fallback_columns() -> Vec<BoardColumn> {
    vec![
        BoardColumn {
            id: BoardState::Todo.fallback_column_id().to_string(),
            name: BoardState::Todo.fallback_column_name().to_string(),
            order: 10,
        },
        BoardColumn {
            id: BoardState::InProgress.fallback_column_id().to_string(),
            name: BoardState::InProgress.fallback_column_name().to_string(),
            order: 20,
        },
        BoardColumn {
            id: BoardState::Done.fallback_column_id().to_string(),
            name: BoardState::Done.fallback_column_name().to_string(),
            order: 30,
        },
    ]
}

fn ensure_items_have_matching_columns(items: &mut [WorkItem], columns: &mut Vec<BoardColumn>) {
    let mut max_order = columns.iter().map(|col| col.order).max().unwrap_or(0);
    let mut column_ids: HashMap<String, String> = columns
        .iter()
        .map(|col| (col.id.clone(), col.id.clone()))
        .collect();
    let mut column_ids_by_name: HashMap<String, String> = columns
        .iter()
        .map(|col| (normalize_name(&col.name), col.id.clone()))
        .collect();

    for item in items {
        if let Some(id) = item.board_column_id.clone() {
            if column_ids.contains_key(&id) {
                continue;
            }

            if let Some(name) = item.board_column_name.clone() {
                let normalized = normalize_name(&name);
                if let Some(existing_id) = column_ids_by_name.get(&normalized) {
                    item.board_column_id = Some(existing_id.clone());
                    continue;
                }

                let column_id = id.clone();
                max_order += 10;
                columns.push(BoardColumn {
                    id: column_id.clone(),
                    name: name.clone(),
                    order: max_order,
                });
                column_ids.insert(column_id.clone(), column_id.clone());
                column_ids_by_name.insert(normalized, column_id);
                continue;
            }
        }

        if let Some(name) = item.board_column_name.clone() {
            let normalized = normalize_name(&name);
            if let Some(existing_id) = column_ids_by_name.get(&normalized) {
                item.board_column_id = Some(existing_id.clone());
                continue;
            }

            let synthetic_id = synthetic_column_id_from_name(&name);
            max_order += 10;
            columns.push(BoardColumn {
                id: synthetic_id.clone(),
                name: name.clone(),
                order: max_order,
            });
            item.board_column_id = Some(synthetic_id.clone());
            column_ids.insert(synthetic_id.clone(), synthetic_id.clone());
            column_ids_by_name.insert(normalized, synthetic_id);
            continue;
        }

        let fallback_id = item.board_state.fallback_column_id().to_string();
        if !column_ids.contains_key(&fallback_id) {
            max_order += 10;
            columns.push(BoardColumn {
                id: fallback_id.clone(),
                name: item.board_state.fallback_column_name().to_string(),
                order: max_order,
            });
            column_ids.insert(fallback_id.clone(), fallback_id.clone());
            column_ids_by_name.insert(
                normalize_name(item.board_state.fallback_column_name()),
                fallback_id.clone(),
            );
        }
        item.board_column_id = Some(fallback_id);
        item.board_column_name = Some(item.board_state.fallback_column_name().to_string());
    }
}

fn sort_items_by_column_and_priority(items: &mut [WorkItem], columns: &[BoardColumn]) {
    let column_rank: HashMap<&str, usize> = columns
        .iter()
        .enumerate()
        .map(|(idx, col)| (col.id.as_str(), idx))
        .collect();

    items.sort_by(|a, b| {
        let rank_a = a
            .board_column_id
            .as_deref()
            .and_then(|id| column_rank.get(id))
            .copied()
            .unwrap_or(usize::MAX);
        let rank_b = b
            .board_column_id
            .as_deref()
            .and_then(|id| column_rank.get(id))
            .copied()
            .unwrap_or(usize::MAX);

        rank_a
            .cmp(&rank_b)
            .then_with(|| match (a.priority, b.priority) {
                (Some(pa), Some(pb)) => pa.cmp(&pb),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            })
            .then_with(|| compare_work_item_ids(&a.id, &b.id))
    });
}

fn normalize_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn compare_work_item_ids(a: &str, b: &str) -> Ordering {
    let a_is_numeric = !a.is_empty() && a.as_bytes().iter().all(u8::is_ascii_digit);
    let b_is_numeric = !b.is_empty() && b.as_bytes().iter().all(u8::is_ascii_digit);

    if a_is_numeric && b_is_numeric {
        let a_normalized = {
            let trimmed = a.trim_start_matches('0');
            if trimmed.is_empty() {
                "0"
            } else {
                trimmed
            }
        };
        let b_normalized = {
            let trimmed = b.trim_start_matches('0');
            if trimmed.is_empty() {
                "0"
            } else {
                trimmed
            }
        };

        return a_normalized
            .len()
            .cmp(&b_normalized.len())
            .then_with(|| a_normalized.cmp(b_normalized));
    }

    a.cmp(b)
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use time::OffsetDateTime;

    use crate::domain::models::{
        BoardColumnAssignment, PullRequestRef, WorkItemCategory, WorkItemPerson, WorkItemRef,
    };

    use super::*;

    #[test]
    fn compare_work_item_ids_uses_numeric_order_for_numeric_ids() {
        assert_eq!(compare_work_item_ids("9", "10"), Ordering::Less);
        assert_eq!(compare_work_item_ids("10", "9"), Ordering::Greater);
        assert_eq!(compare_work_item_ids("010", "10"), Ordering::Equal);
    }

    #[test]
    fn compare_work_item_ids_falls_back_to_lexicographic_for_non_numeric_ids() {
        assert_eq!(compare_work_item_ids("abc-10", "abc-2"), Ordering::Less);
        assert_eq!(
            compare_work_item_ids("owner/repo#2", "owner/repo#10"),
            Ordering::Greater
        );
    }

    #[derive(Clone, Default)]
    struct MockProvider {
        ids: Vec<String>,
        items: Vec<WorkItem>,
        columns: Vec<BoardColumn>,
        assignments: HashMap<String, BoardColumnAssignment>,
    }

    #[async_trait]
    impl WorkItemProvider for MockProvider {
        async fn get_iterations(&self) -> Result<Vec<Iteration>, WorkItemError> {
            Ok(vec![])
        }

        async fn query_work_item_ids(
            &self,
            _iteration_path: Option<&str>,
            _team: Option<&str>,
        ) -> Result<Vec<String>, WorkItemError> {
            Ok(self.ids.clone())
        }

        async fn get_work_items(&self, _ids: &[String]) -> Result<Vec<WorkItem>, WorkItemError> {
            Ok(self.items.clone())
        }

        async fn get_board_columns(
            &self,
            _iteration_path: Option<&str>,
            _team: Option<&str>,
        ) -> Vec<BoardColumn> {
            self.columns.clone()
        }

        async fn get_taskboard_column_assignments(
            &self,
            _iteration_path: Option<&str>,
            _team: Option<&str>,
        ) -> HashMap<String, BoardColumnAssignment> {
            self.assignments.clone()
        }

        async fn get_work_item_comments(
            &self,
            _work_item_id: &str,
        ) -> Result<Vec<crate::domain::models::WorkItemComment>, WorkItemError> {
            Ok(vec![])
        }

        async fn format_work_item_for_llm(
            &self,
            _work_item_id: &str,
        ) -> Result<(String, bool), WorkItemError> {
            Ok((String::new(), false))
        }

        async fn fetch_image(&self, _image_url: &str) -> Result<WorkItemImage, WorkItemError> {
            Ok(WorkItemImage {
                bytes: vec![],
                content_type: Some("image/png".to_string()),
            })
        }

        async fn move_work_item_to_column(
            &self,
            _work_item_id: &str,
            _target_column_name: &str,
            _iteration_path: Option<&str>,
            _team: Option<&str>,
        ) -> Result<(), WorkItemError> {
            Ok(())
        }
    }

    fn make_item(id: &str, board_state: BoardState, priority: Option<i32>) -> WorkItem {
        WorkItem {
            id: id.to_string(),
            title: format!("Work item {id}"),
            board_state,
            board_column_id: None,
            board_column_name: None,
            category: WorkItemCategory::Task,
            state_name: "Active".to_string(),
            priority,
            assigned_to: Some(WorkItemPerson {
                display_name: "User".to_string(),
                unique_name: Some("user@example.com".to_string()),
                image_url: None,
            }),
            created_by: None,
            description: None,
            description_rendered_html: None,
            acceptance_criteria: None,
            iteration_path: None,
            area_path: None,
            tags: vec![],
            parent: Some(WorkItemRef {
                id: "0".to_string(),
                title: None,
            }),
            related: vec![],
            pull_requests: vec![PullRequestRef {
                id: "1".to_string(),
                repository_id: "repo".to_string(),
                project_id: "project".to_string(),
                url: "https://example.invalid/pr/1".to_string(),
            }],
            url: format!("https://example.invalid/{id}"),
            created_at: OffsetDateTime::now_utc(),
            changed_at: OffsetDateTime::now_utc(),
        }
    }

    #[tokio::test]
    async fn uses_fallback_columns_when_board_columns_are_unavailable() {
        let provider = MockProvider {
            ids: vec!["1".to_string()],
            items: vec![make_item("1", BoardState::InProgress, Some(2))],
            ..Default::default()
        };
        let service = WorkItemServiceImpl::new(Arc::new(provider));

        let board = service.get_board_data(None, None).await.unwrap();

        assert_eq!(board.columns.len(), 3);
        assert_eq!(board.columns[0].id, "todo");
        assert_eq!(board.columns[1].id, "inProgress");
        assert_eq!(board.columns[2].id, "done");
        assert_eq!(
            board.items[0].board_column_id.as_deref(),
            Some("inProgress")
        );
        assert_eq!(
            board.items[0].board_column_name.as_deref(),
            Some("In Progress")
        );
    }

    #[tokio::test]
    async fn appends_unknown_assigned_column_to_the_end() {
        let mut assignments = HashMap::new();
        assignments.insert(
            "1".to_string(),
            BoardColumnAssignment {
                column_id: Some("custom-review".to_string()),
                column_name: "In Review".to_string(),
            },
        );

        let provider = MockProvider {
            ids: vec!["1".to_string()],
            items: vec![make_item("1", BoardState::InProgress, Some(2))],
            columns: vec![BoardColumn {
                id: "todo".to_string(),
                name: "To Do".to_string(),
                order: 10,
            }],
            assignments,
            ..Default::default()
        };
        let service = WorkItemServiceImpl::new(Arc::new(provider));

        let board = service.get_board_data(None, None).await.unwrap();

        assert!(board
            .columns
            .iter()
            .any(|col| { col.id == "custom-review" && col.name == "In Review" && col.order > 10 }));
        assert_eq!(
            board.items[0].board_column_id.as_deref(),
            Some("custom-review")
        );
    }

    #[tokio::test]
    async fn sorts_items_by_column_order_then_priority() {
        let provider = MockProvider {
            ids: vec!["1".to_string(), "2".to_string(), "3".to_string()],
            items: vec![
                make_item("1", BoardState::InProgress, Some(2)),
                make_item("2", BoardState::InProgress, Some(1)),
                make_item("3", BoardState::Todo, Some(3)),
            ],
            columns: vec![
                BoardColumn {
                    id: "todo".to_string(),
                    name: "To Do".to_string(),
                    order: 10,
                },
                BoardColumn {
                    id: "inProgress".to_string(),
                    name: "In Progress".to_string(),
                    order: 20,
                },
            ],
            ..Default::default()
        };
        let service = WorkItemServiceImpl::new(Arc::new(provider));

        let board = service.get_board_data(None, None).await.unwrap();
        let item_ids: Vec<_> = board.items.iter().map(|item| item.id.as_str()).collect();

        assert_eq!(item_ids, vec!["3", "2", "1"]);
    }

    #[tokio::test]
    async fn move_work_item_validates_input() {
        let service = WorkItemServiceImpl::new(Arc::new(MockProvider::default()));

        let empty_id_err = service
            .move_work_item_to_column("   ", "Done", None, None)
            .await
            .unwrap_err();
        assert!(matches!(empty_id_err, WorkItemError::InvalidInput(_)));

        let empty_column_err = service
            .move_work_item_to_column("123", "   ", None, None)
            .await
            .unwrap_err();
        assert!(matches!(empty_column_err, WorkItemError::InvalidInput(_)));
    }
}
