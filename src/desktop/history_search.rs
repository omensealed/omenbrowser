use iced::Task;

use super::{DesktopApp, HistorySearchMessage, Message};
use crate::history_search::{search_persisted_local_history, LocalHistorySearchQuery};

impl DesktopApp {
    pub(super) fn dispatch_history_search_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::HistorySearch(message) => match *message {
                HistorySearchMessage::Submit(query) => Ok(self.submit_local_history_search(query)),
                HistorySearchMessage::Completed { generation, result } => {
                    Ok(self.complete_local_history_search(generation, result))
                }
            },
            _ => Err(message),
        }
    }

    fn submit_local_history_search(&mut self, query: LocalHistorySearchQuery) -> Task<Message> {
        let Some(job) = self.history_search.submit(query) else {
            self.app.status.task = "history search queued; previous scan is finishing".into();
            return Task::none();
        };
        self.app.status.task = "searching bounded local history".into();
        self.local_history_search_task(job)
    }

    fn complete_local_history_search(
        &mut self,
        generation: u64,
        result: Result<crate::history_search::LocalHistorySearchPage, String>,
    ) -> Task<Message> {
        let Some(next) = self.history_search.complete(generation, result) else {
            if self.history_search.error.is_some() {
                self.app.status.task = "local history search failed".into();
            } else if let Some(page) = &self.history_search.result {
                self.app.status.task = format!(
                    "local history search: {} result(s), {} item(s) examined",
                    page.results.len(),
                    page.scanned_items
                );
            }
            return Task::none();
        };
        self.app.status.task = "searching latest queued local-history query".into();
        self.local_history_search_task(next)
    }

    fn local_history_search_task(
        &self,
        job: super::history_search_state::LocalHistorySearchJob,
    ) -> Task<Message> {
        let generation = job.generation;
        let query = job.query;
        let message_store = self.app.message_store.clone();
        #[cfg(feature = "chat-client")]
        let chat_path = self.omenchat.chat_store.as_ref().map(|_| {
            self.app
                .paths
                .identity_storage_root()
                .join("plugins")
                .join(crate::plugins::BUILTIN_OMENCHAT_PLUGIN_ID)
                .join("chat.sqlite")
        });
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    search_persisted_local_history(
                        &message_store,
                        #[cfg(feature = "chat-client")]
                        chat_path.as_deref(),
                        &query,
                    )
                    .map_err(|error| error.to_string())
                })
                .await
                .unwrap_or_else(|error| Err(format!("local history search task failed: {error}")))
            },
            move |result| {
                Message::HistorySearch(Box::new(HistorySearchMessage::Completed {
                    generation,
                    result,
                }))
            },
        )
    }
}
