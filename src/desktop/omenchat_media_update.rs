use iced::Task;

use super::{DesktopApp, Message};

impl DesktopApp {
    pub(super) fn dispatch_omenchat_media_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::OpenCachedOmenChatMedia(path) => {
                self.update_open_cached_omenchat_media(path);
                Ok(Task::none())
            }
            Message::LoadOmenChatMedia(url) => Ok(self.update_load_omenchat_media(url)),
            Message::FetchOmenChatUploadResource {
                session_id,
                resource_id,
            } => {
                self.update_fetch_omenchat_upload_resource(session_id, resource_id);
                Ok(Task::none())
            }
            Message::PickOmenChatUpload(session_id) => {
                Ok(self.update_pick_omenchat_upload(session_id))
            }
            Message::OmenChatUploadPicked { session_id, result } => {
                self.update_omenchat_upload_picked(session_id, result);
                Ok(Task::none())
            }
            Message::OmenChatGifFramesLoaded { path, result } => {
                self.update_omenchat_gif_frames_loaded(path, result);
                Ok(Task::none())
            }
            Message::OmenChatMediaLoaded { url, result } => {
                Ok(self.update_omenchat_media_loaded(url, result))
            }
            _ => Err(message),
        }
    }
}
