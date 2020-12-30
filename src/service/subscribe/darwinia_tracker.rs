use std::sync::Arc;
use crate::{
    api::Darwinia, error::Result
};
use substrate_subxt::BlockNumber;
use substrate_subxt::sp_runtime::generic::Header;
use substrate_subxt::sp_runtime::traits::BlakeTwo256;
use std::time::Duration;
use tokio::time::delay_for;

/// DarwiniaTracker
pub struct DarwiniaBlockTracker {
    darwinia: Arc<Darwinia>,
    next_block: u32,
}

impl DarwiniaBlockTracker {
    /// new
    pub fn new(darwinia: Arc<Darwinia>, scan_from: u32) -> Self {
        Self {
            darwinia,
            next_block: scan_from
        }
    }

    /// get next block
    pub async fn next_block(&mut self) -> Header<u32, BlakeTwo256> {
        loop {
            match self.get_next_block().await {
                Ok(result) => {
                    if let Some(header) = result {
                        return header;
                    } else {
                        delay_for(Duration::from_secs(6)).await;
                    }
                },
                Err(err) => {
                    error!("Encounter error when track next block: {:#?}", err);
                    delay_for(Duration::from_secs(30)).await;
                }
            }
        }
    }

    async fn get_next_block(&mut self) -> Result<Option<Header<u32, BlakeTwo256>>> {
        let finalized_block_hash = self.darwinia.client.finalized_head().await?;
        match self.darwinia.client.block(Some(finalized_block_hash)).await? {
            Some(finalized_block) => {
                let finalized_block_number = finalized_block.block.header.number;
                if self.next_block > finalized_block_number {
                    Ok(None)
                } else {
                    let block = BlockNumber::from(self.next_block);

                    match self.darwinia.client.block_hash(Some(block)).await? {
                        Some(block_hash) => {
                            match self.darwinia.client.header(Some(block_hash)).await? {
                                Some(header) => {
                                    self.next_block += 1;
                                    Ok(Some(header))
                                },
                                None => Ok(None)
                            }
                        },
                        None => Ok(None)
                    }
                }
            },
            None => Ok(None)
        }
    }

}