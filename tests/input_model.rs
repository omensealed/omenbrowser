use omenbrowser_rs::input::{InputBuffer, InputState, InputTarget};

#[test]
fn input_buffer_edits_at_cursor() {
    let mut buffer = InputBuffer::new("abc");
    buffer.move_left();
    buffer.insert_char('X');
    assert_eq!(buffer.as_str(), "abXc");
    buffer.backspace();
    assert_eq!(buffer.as_str(), "abc");
    buffer.delete();
    assert_eq!(buffer.as_str(), "ab");
}

#[test]
fn input_buffer_handles_utf8_boundaries() {
    let mut buffer = InputBuffer::new("aé");
    buffer.move_left();
    assert_eq!(buffer.cursor(), 1);
    buffer.delete();
    assert_eq!(buffer.as_str(), "a");
}

#[test]
fn input_buffer_inserts_text_at_a_utf8_cursor_boundary() {
    let mut buffer = InputBuffer::new("aé");
    buffer.move_left();
    buffer.insert_str("界!");
    assert_eq!(buffer.as_str(), "a界!é");
    assert_eq!(buffer.cursor(), "a界!".len());
}

#[test]
fn input_buffer_sets_cursor_by_character_index() {
    let mut buffer = InputBuffer::new("aéz");
    buffer.set_cursor_char_index(2);
    assert_eq!(buffer.cursor(), "aé".len());
    buffer.insert_char('!');
    assert_eq!(buffer.as_str(), "aé!z");

    buffer.set_cursor_char_index(99);
    assert_eq!(buffer.cursor(), buffer.as_str().len());
}

#[test]
fn input_state_tracks_submit_and_cancel_originals() {
    let mut input = InputState::default();
    input.begin(InputTarget::BrowserAddress { tab_id: 7 }, "mock.node:/");
    input.insert_char('x');
    let cancelled = input.cancel().expect("cancelled");
    assert_eq!(cancelled.1, "mock.node:/");

    input.begin(InputTarget::MessageBody { conversation_id: 3 }, "hi");
    input.insert_char('!');
    let submitted = input.take_submitted().expect("submitted");
    assert_eq!(submitted.1, "hi!");
}
