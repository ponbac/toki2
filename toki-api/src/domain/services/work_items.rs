use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{
    models::{BoardState, Iteration, WorkItem},
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

    async fn get_board_items(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<Vec<WorkItem>, WorkItemError> {
        let ids = self
            .provider
            .query_work_item_ids(iteration_path, team)
            .await?;

        if ids.is_empty() {
            return Ok(vec![]);
        }

        let mut items = self.provider.get_work_items(&ids).await?;

        // Re-map board states using sprint taskboard column assignments.
        // The taskboard API gives us the actual column each work item is in,
        // which may differ from System.State when the board is customized
        // (e.g. "Ready for development" column maps to Active state in ADO,
        // but should be Todo in our board).
        let column_map = self
            .provider
            .get_taskboard_columns(iteration_path, team)
            .await;

        if !column_map.is_empty() {
            for item in &mut items {
                if let Some(column) = column_map.get(&item.id) {
                    item.board_state = BoardState::from_taskboard_column(column);
                }
            }
        }

        // Business logic: sort by board state (Todo → InProgress → Done),
        // then by priority ascending (1 = highest), with None last.
        items.sort_by(|a, b| {
            a.board_state
                .cmp(&b.board_state)
                .then_with(|| match (a.priority, b.priority) {
                    (Some(pa), Some(pb)) => pa.cmp(&pb),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                })
        });

        Ok(items)
    }

    async fn format_work_item_for_llm(
        &self,
        work_item_id: &str,
    ) -> Result<(String, bool), WorkItemError> {
        self.provider.format_work_item_for_llm(work_item_id).await
    }
}
