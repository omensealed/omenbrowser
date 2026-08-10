use crate::history_search::{
    LocalHistorySearchPage, LocalHistorySearchQuery, LocalHistorySourceFilter,
    LOCAL_HISTORY_SEARCH_QUERY_MAX_BYTES,
};

#[derive(Clone, Debug)]
pub(in crate::desktop) struct LocalHistorySearchJob {
    pub(in crate::desktop) generation: u64,
    pub(in crate::desktop) query: LocalHistorySearchQuery,
}

#[derive(Default)]
pub(in crate::desktop) struct LocalHistorySearchDesktopState {
    generation: u64,
    active_generation: Option<u64>,
    pending: Option<LocalHistorySearchJob>,
    pub(in crate::desktop) visible: bool,
    pub(in crate::desktop) draft: String,
    pub(in crate::desktop) source: LocalHistorySourceFilter,
    pub(in crate::desktop) result: Option<LocalHistorySearchPage>,
    pub(in crate::desktop) error: Option<String>,
}

impl LocalHistorySearchDesktopState {
    pub(in crate::desktop) fn toggle_visible(&mut self) {
        self.visible = !self.visible;
    }

    pub(in crate::desktop) fn update_draft(&mut self, value: String) {
        if value.len() <= LOCAL_HISTORY_SEARCH_QUERY_MAX_BYTES
            && !value.chars().any(char::is_control)
        {
            self.draft = value;
            self.error = None;
        }
    }

    pub(in crate::desktop) fn cycle_source(&mut self) {
        self.source = match self.source {
            LocalHistorySourceFilter::All => LocalHistorySourceFilter::Lxmf,
            LocalHistorySourceFilter::Lxmf => LocalHistorySourceFilter::OmenChat,
            LocalHistorySourceFilter::OmenChat => LocalHistorySourceFilter::All,
        };
    }

    pub(in crate::desktop) fn current_query(&self) -> LocalHistorySearchQuery {
        LocalHistorySearchQuery {
            text: self.draft.clone(),
            source: self.source,
            ..LocalHistorySearchQuery::default()
        }
    }

    pub(in crate::desktop) fn is_active(&self) -> bool {
        self.active_generation.is_some()
    }

    pub(in crate::desktop) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(in crate::desktop) fn submit(
        &mut self,
        query: LocalHistorySearchQuery,
    ) -> Option<LocalHistorySearchJob> {
        self.generation = self.generation.wrapping_add(1).max(1);
        let job = LocalHistorySearchJob {
            generation: self.generation,
            query,
        };
        self.error = None;
        if self.active_generation.is_some() {
            self.pending = Some(job);
            None
        } else {
            self.active_generation = Some(job.generation);
            Some(job)
        }
    }

    pub(in crate::desktop) fn complete(
        &mut self,
        generation: u64,
        result: Result<LocalHistorySearchPage, String>,
    ) -> Option<LocalHistorySearchJob> {
        if self.active_generation != Some(generation) {
            return None;
        }
        self.active_generation = None;
        if self.pending.is_none() && generation == self.generation {
            match result {
                Ok(page) => {
                    self.result = Some(page);
                    self.error = None;
                }
                Err(error) => {
                    self.result = None;
                    self.error = Some(error);
                }
            }
        }
        let next = self.pending.take();
        if let Some(next) = &next {
            self.active_generation = Some(next.generation);
        }
        next
    }

    pub(in crate::desktop) fn cancel_for_shutdown(&mut self) {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.active_generation = None;
        self.pending = None;
    }

    #[cfg(test)]
    fn active_generation(&self) -> Option<u64> {
        self.active_generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(text: &str) -> LocalHistorySearchQuery {
        LocalHistorySearchQuery {
            text: text.into(),
            ..LocalHistorySearchQuery::default()
        }
    }

    #[test]
    fn owner_runs_one_job_and_keeps_only_the_newest_pending_query() {
        let mut state = LocalHistorySearchDesktopState::default();
        let first = state.submit(query("first")).expect("first job");
        assert!(state.submit(query("second")).is_none());
        assert!(state.submit(query("third")).is_none());
        assert_eq!(state.active_generation(), Some(first.generation));

        let next = state
            .complete(first.generation, Ok(LocalHistorySearchPage::default()))
            .expect("newest pending job");
        assert_eq!(next.query.text, "third");
        assert!(state.result.is_none(), "superseded result must be ignored");
        assert_eq!(state.active_generation(), Some(next.generation));
    }

    #[test]
    fn stale_completion_cannot_replace_current_result_or_owner() {
        let mut state = LocalHistorySearchDesktopState::default();
        let current = state.submit(query("current")).expect("current job");
        assert!(state
            .complete(
                current.generation.wrapping_add(10),
                Err("stale failure".into())
            )
            .is_none());
        assert_eq!(state.active_generation(), Some(current.generation));
        assert!(state.error.is_none());

        let page = LocalHistorySearchPage {
            scanned_items: 7,
            ..LocalHistorySearchPage::default()
        };
        assert!(state.complete(current.generation, Ok(page)).is_none());
        assert_eq!(
            state.result.as_ref().map(|result| result.scanned_items),
            Some(7)
        );
    }

    #[test]
    fn shutdown_discards_pending_and_rejects_late_completion() {
        let mut state = LocalHistorySearchDesktopState::default();
        let active = state.submit(query("active")).expect("active job");
        assert!(state.submit(query("pending")).is_none());
        state.cancel_for_shutdown();
        assert!(state
            .complete(active.generation, Ok(LocalHistorySearchPage::default()))
            .is_none());
        assert!(state.result.is_none());
        assert!(state.error.is_none());
        assert_eq!(state.active_generation(), None);
    }

    #[test]
    fn draft_and_source_controls_are_bounded_without_starting_work() {
        let mut state = LocalHistorySearchDesktopState::default();
        state.update_draft("ridge map".into());
        assert_eq!(state.current_query().text, "ridge map");
        state.update_draft("x".repeat(LOCAL_HISTORY_SEARCH_QUERY_MAX_BYTES + 1));
        assert_eq!(state.current_query().text, "ridge map");
        assert_eq!(state.source, LocalHistorySourceFilter::All);
        state.cycle_source();
        assert_eq!(state.source, LocalHistorySourceFilter::Lxmf);
        state.cycle_source();
        assert_eq!(state.source, LocalHistorySourceFilter::OmenChat);
        state.cycle_source();
        assert_eq!(state.source, LocalHistorySourceFilter::All);
        assert!(!state.is_active());
    }

    #[test]
    fn search_panel_is_collapsed_by_default_and_can_be_reopened() {
        let mut state = LocalHistorySearchDesktopState::default();
        assert!(!state.visible);
        state.toggle_visible();
        assert!(state.visible);
        state.toggle_visible();
        assert!(!state.visible);
    }
}
