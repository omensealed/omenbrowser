use crate::storage::settings::RuntimeBackendSetting;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceEditField {
    ProfileName,
    TcpHost,
    TcpPort,
    TcpIfacNetworkName,
    TcpIfacPassphrase,
    I2pPeers,
    RNodeDevicePort,
    RNodeFrequency,
    RNodeBandwidth,
    RNodeTxPower,
    RNodeSpreadingFactor,
    RNodeCodingRate,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputBuffer {
    text: String,
    cursor: usize,
}

impl InputBuffer {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self { text, cursor }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
    }

    pub fn set_cursor_char_index(&mut self, char_index: usize) {
        self.cursor = self
            .text
            .char_indices()
            .nth(char_index)
            .map(|(index, _)| index)
            .unwrap_or(self.text.len());
    }

    pub fn insert_char(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn insert_str(&mut self, text: &str) {
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    pub fn backspace(&mut self) {
        let Some(previous) = self.previous_boundary() else {
            return;
        };
        self.text.drain(previous..self.cursor);
        self.cursor = previous;
    }

    pub fn delete(&mut self) {
        let Some(next) = self.next_boundary() else {
            return;
        };
        self.text.drain(self.cursor..next);
    }

    pub fn move_left(&mut self) {
        if let Some(previous) = self.previous_boundary() {
            self.cursor = previous;
        }
    }

    pub fn move_right(&mut self) {
        if let Some(next) = self.next_boundary() {
            self.cursor = next;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    pub fn display_with_cursor(&self) -> String {
        let mut rendered = self.text.clone();
        rendered.insert(self.cursor, '|');
        rendered
    }

    fn previous_boundary(&self) -> Option<usize> {
        self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(index, _)| index)
    }

    fn next_boundary(&self) -> Option<usize> {
        self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| self.cursor + index)
            .or_else(|| (self.cursor < self.text.len()).then_some(self.text.len()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputTarget {
    BrowserAddress {
        tab_id: u64,
    },
    BrowserField {
        tab_id: u64,
        name: String,
    },
    MessageTitle {
        conversation_id: u64,
    },
    MessageBody {
        conversation_id: u64,
    },
    PluginInstallPath,
    PluginRemoveConfirm {
        plugin_id: String,
    },
    RuntimeBackendConfirm {
        backend: RuntimeBackendSetting,
    },
    InterfaceDeleteConfirm {
        profile_id: String,
    },
    InterfaceField {
        profile_id: String,
        field: InterfaceEditField,
    },
    SettingsThemeName,
    SettingsDefaultStartPage,
    SettingsBrowserFormMaxAgeSecs,
    SettingsLxmfSyncIntervalSecs,
    SettingsLxmfSyncLimit,
    SettingsPreferredPropagationNode,
    SettingsIdentityPath,
    SettingsReticulumConfigPath,
    DiagnosticsKnownDestinationsPath,
    OperationsSearch,
    SettingsLogMaxBytes,
    SettingsLogRetainFiles,
    SettingsLogLoadRecentEntries,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveInput {
    pub target: InputTarget,
    pub buffer: InputBuffer,
    pub original: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputState {
    pub active: Option<ActiveInput>,
}

impl InputState {
    pub fn begin(&mut self, target: InputTarget, text: impl Into<String>) {
        let text = text.into();
        self.active = Some(ActiveInput {
            target,
            buffer: InputBuffer::new(text.clone()),
            original: text,
        });
    }

    pub fn cancel(&mut self) -> Option<(InputTarget, String)> {
        self.active
            .take()
            .map(|active| (active.target, active.original))
    }

    pub fn take_submitted(&mut self) -> Option<(InputTarget, String)> {
        self.active
            .take()
            .map(|active| (active.target, active.buffer.as_str().to_string()))
    }

    pub fn insert_char(&mut self, ch: char) -> bool {
        let mut encoded = [0; 4];
        self.insert_text(ch.encode_utf8(&mut encoded))
    }

    pub fn insert_text(&mut self, text: &str) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        active.buffer.insert_str(text);
        true
    }

    pub fn backspace(&mut self) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        active.buffer.backspace();
        true
    }

    pub fn delete(&mut self) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        active.buffer.delete();
        true
    }

    pub fn move_left(&mut self) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        active.buffer.move_left();
        true
    }

    pub fn move_right(&mut self) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        active.buffer.move_right();
        true
    }

    pub fn move_home(&mut self) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        active.buffer.move_home();
        true
    }

    pub fn move_end(&mut self) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        active.buffer.move_end();
        true
    }
}
