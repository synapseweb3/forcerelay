use crate::chain::handle::ChainHandle;
use crate::event::monitor::EventBatch;

pub fn handle_event_batch<ChainA: ChainHandle, ChainB: ChainHandle>(
    _eth_chain: &ChainA,
    _ckb_chain: &ChainB,
    _event_batch: &EventBatch,
) {
}
