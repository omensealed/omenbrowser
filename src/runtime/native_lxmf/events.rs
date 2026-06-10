use crate::runtime::event::MessageBusEvent;

#[derive(Clone, Debug, PartialEq)]
pub struct NativeLxmfEvent {
    pub event: MessageBusEvent,
}
