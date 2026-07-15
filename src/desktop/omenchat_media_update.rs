use iced::Task;

use super::{DesktopApp, Message, OmenChatMediaCompletionMessage, OmenChatMessage};

impl DesktopApp {
    pub(super) fn dispatch_omenchat_media_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::OmenChat(OmenChatMessage::OpenCachedMedia(path)) => {
                self.update_open_cached_omenchat_media(path);
                Ok(Task::none())
            }
            Message::OmenChat(OmenChatMessage::LoadMedia(url)) => {
                Ok(self.update_load_omenchat_media(url))
            }
            Message::OmenChat(OmenChatMessage::FetchUploadResource {
                session_id,
                resource_id,
            }) => {
                self.update_fetch_omenchat_upload_resource(session_id, resource_id);
                Ok(Task::none())
            }
            Message::OmenChat(OmenChatMessage::PickUpload(session_id)) => {
                Ok(self.update_pick_omenchat_upload(session_id))
            }
            Message::OmenChatMediaCompletion(OmenChatMediaCompletionMessage::UploadPicked {
                session_id,
                result,
            }) => {
                self.update_omenchat_upload_picked(session_id, result);
                Ok(Task::none())
            }
            Message::OmenChatMediaCompletion(OmenChatMediaCompletionMessage::GifFramesLoaded {
                path,
                result,
            }) => {
                self.update_omenchat_gif_frames_loaded(path, result);
                Ok(Task::none())
            }
            Message::OmenChatMediaCompletion(OmenChatMediaCompletionMessage::CacheCompleted(
                completion,
            )) => Ok(self.update_omenchat_media_cache_completed(
                completion.session_id,
                completion.cache_key,
                completion.generation,
                completion.result,
            )),
            Message::OmenChatMediaCompletion(OmenChatMediaCompletionMessage::StaleMediaRemoved) => {
                Ok(Task::none())
            }
            Message::OmenChatMediaCompletion(OmenChatMediaCompletionMessage::MediaLoaded {
                url,
                result,
            }) => Ok(self.update_omenchat_media_loaded(url, result)),
            _ => Err(message),
        }
    }
}
