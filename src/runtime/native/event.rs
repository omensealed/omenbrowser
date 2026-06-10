use crate::runtime::event::RuntimeBusEvent;

#[derive(Clone, Debug, PartialEq)]
pub struct NativeRuntimeEvent {
    pub event: RuntimeBusEvent,
}
