use std::fmt::Display;

use bridge_e2e_traits::client::EcdsaClient;
use support_tracker::Tracker;

use crate::{
    ecdsa::{
        collected_enough_authorities_change_signatures::CollectedEnoughAuthoritiesChangeSignaturesRunner,
        collected_enough_new_message_root_signatures::CollectedEnoughNewMessageRootSignaturesRunner,
        collecting_authorities_change_signatures::CollectingAuthoritiesChangeSignaturesRunner,
        collecting_new_message_root_signatures::CollectingNewMessageRootSignaturesRunner,
    },
    error::{RelayError, RelayResult},
};

use super::types::EcdsaSource;

#[derive(Clone, Copy, Debug)]
pub enum EcdsaScanType {
    CollectingMessage,
    CollectedMessage,
    CollectingAuthority,
    CollectedAuthority,
}

impl Display for EcdsaScanType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            EcdsaScanType::CollectedMessage => "collectedMessages",
            EcdsaScanType::CollectingMessage => "collectingMessages",
            EcdsaScanType::CollectingAuthority => "collectingAuthorities",
            EcdsaScanType::CollectedAuthority => "collectedAuthorities",
        };
        write!(f, "{}", s)
    }
}

#[async_trait::async_trait]
pub trait EcdsaScanner<T: EcdsaClient> {
    async fn get_ecdsa_source(&self) -> RelayResult<EcdsaSource<T>>;

    async fn start(&self, tracker: Tracker, scan_type: EcdsaScanType) {
        while let Err(err) = self.run(tracker.clone(), scan_type).await {
            tracing::error!(
                target: "relay-e2e",
                "[Darwinia][ECDSA] An error occurred while processing the extrinsics: {:?}",
                err
            );
            // Prevent too fast refresh errors
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    }

    async fn run(&self, tracker: Tracker, scan_type: EcdsaScanType) -> RelayResult<()> {
        if !tracker
            .is_running()
            .map_err(|e| RelayError::Custom(format!("{e:?}")))?
        {
            return Ok(());
        }

        let mut source = self.get_ecdsa_source().await?;
        loop {
            let from = tracker
                .current()
                .await
                .map_err(|e| RelayError::Custom(format!("{}", e)))?;
            tracing::info!(
                target: "relay-e2e",
                "[Darwinia][ECDSA][{}] Track scan block: {} ",
                scan_type,
                from,
            );
            source.block = Some(from as u32);

            let finished_block = match scan_type {
                EcdsaScanType::CollectingMessage => {
                    let runner = CollectingNewMessageRootSignaturesRunner::new(source.clone());
                    runner.start().await?
                }
                EcdsaScanType::CollectedMessage => {
                    let mut runner = CollectedEnoughNewMessageRootSignaturesRunner::new(
                        source.clone(),
                        source.minimal_interval,
                    );
                    runner.start().await?
                }
                EcdsaScanType::CollectingAuthority => {
                    let runner = CollectingAuthoritiesChangeSignaturesRunner::new(source.clone());
                    runner.start().await?
                }
                EcdsaScanType::CollectedAuthority => {
                    let runner =
                        CollectedEnoughAuthoritiesChangeSignaturesRunner::new(source.clone());
                    runner.start().await?
                }
            };
            if finished_block.is_some() {
                tracker
                    .finish(finished_block.unwrap() as usize)
                    .map_err(|e| RelayError::Custom(format!("{}", e)))?;
            }

            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    }
}

#[async_trait::async_trait]
impl<T: EcdsaClient> EcdsaScanner<T> for EcdsaSource<T> {
    async fn get_ecdsa_source(&self) -> RelayResult<EcdsaSource<T>> {
        Ok(self.clone())
    }
}
