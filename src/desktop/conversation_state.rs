use std::collections::{HashMap, HashSet};

use iced::widget::{scrollable, text_editor};

pub(in crate::desktop) struct ConversationDesktopState {
    pub(in crate::desktop) body_editors: HashMap<u64, text_editor::Content>,
    pub(in crate::desktop) message_counts: HashMap<u64, usize>,
    pub(in crate::desktop) scroll_offsets: HashMap<u64, scrollable::RelativeOffset>,
    pub(in crate::desktop) scroll_restore_locks: HashSet<u64>,
}
