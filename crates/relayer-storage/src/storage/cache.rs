use std::sync::RwLock;

use crate::Slot;

#[derive(Default)]
pub(crate) struct Cache {
    pub(crate) base_beacon_header_slot: RwLock<Option<Slot>>,
}
