use client_crab::client::CrabClient;
use client_darwinia::client::DarwiniaClient;
use lifeline::{Lifeline, Service, Task};
use subquery_s2s::types::RelayBlockOrigin;

use feemarket_s2s::relay::basic::BasicRelayStrategy;
use relay_s2s::message::{BridgeSolochainDeliveryRunner, BridgeSolochainReceivingRunner};
use relay_s2s::types::{MessageDeliveryInput, MessageReceivingInput};
use support_common::config::{Config, Names};
use support_lifeline::service::BridgeService;

use crate::bridge::{BridgeBus, BridgeConfig};

#[derive(Debug)]
pub struct DarwiniaToCrabMessageRelayService {
    _greet_delivery: Lifeline,
    _greet_receiving: Lifeline,
}

impl BridgeService for DarwiniaToCrabMessageRelayService {}

impl Service for DarwiniaToCrabMessageRelayService {
    type Bus = BridgeBus;
    type Lifeline = color_eyre::Result<Self>;

    fn spawn(_bus: &Self::Bus) -> Self::Lifeline {
        let _greet_delivery = Self::try_task(
            "darwinia-to-crab-message-delivery-service",
            async move {
                while let Err(e) = start_delivery().await {
                    tracing::error!(
                        target: "darwinia-crab",
                        "[message-relay] [darwinia-to-crab] An error occurred for message delivery relay {:?}",
                        e,
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    tracing::info!(
                        target: "darwinia-crab",
                        "[message-relay] [darwinia-to-crab] Try to restart message delivery relay service.",
                    );
                }
                Ok(())
            },
        );
        let _greet_receiving = Self::try_task(
            "darwinia-to-crab-message-receiving-service",
            async move {
                while let Err(e) = start_receiving().await {
                    tracing::error!(
                        target: "darwinia-crab",
                        "[message-relay] [darwinia-to-crab] An error occurred for message receiving relay {:?}",
                        e,
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    tracing::info!(
                        target: "darwinia-crab",
                        "[message-relay] [darwinia-to-crab] Try to restart message receiving relay service.",
                    );
                }
                Ok(())
            },
        );
        Ok(Self {
            _greet_delivery,
            _greet_receiving,
        })
    }
}

async fn message_input() -> color_eyre::Result<MessageReceivingInput<DarwiniaClient, CrabClient>> {
    let bridge_config: BridgeConfig = Config::restore(Names::BridgeDarwiniaCrab)?;
    let relay_config = bridge_config.relay;

    let client_darwinia = bridge_config.darwinia.to_darwinia_client().await?;
    let client_crab = bridge_config.crab.to_crab_client().await?;

    let config_index = bridge_config.index;
    let subquery_crab = config_index.to_crab_subquery();
    let subquery_darwinia = config_index.to_darwinia_subquery();

    let lanes = relay_config.raw_lanes();

    let input = MessageReceivingInput {
        lanes,
        relayer_account: client_darwinia.account().account_id().clone(),
        client_source: client_darwinia,
        client_target: client_crab,
        subquery_source: subquery_darwinia,
        subquery_target: subquery_crab,
    };
    Ok(input)
}

async fn start_delivery() -> color_eyre::Result<()> {
    tracing::info!(
        target: "darwinia-crab",
        "[message-delivery] [delivery-darwinia-to-crab] SERVICE RESTARTING..."
    );
    let input = message_input().await?;
    let relay_strategy = BasicRelayStrategy::new(
        input.client_source.clone(),
        input.client_source.account().account_id().clone(),
    );
    let input = MessageDeliveryInput {
        lanes: input.lanes,
        nonces_limit: 11,
        relayer_account: input.relayer_account,
        client_source: input.client_source,
        client_target: input.client_target,
        subquery_source: input.subquery_source,
        subquery_target: input.subquery_target,
        relay_block_origin: RelayBlockOrigin::BridgeCrab,
        relay_strategy,
    };
    let runner = BridgeSolochainDeliveryRunner::new(input);
    Ok(runner.start().await?)
}

async fn start_receiving() -> color_eyre::Result<()> {
    tracing::info!(
        target: "darwinia-crab",
        "[message-receiving] [receiving-darwinia-to-crab] SERVICE RESTARTING..."
    );
    let input = message_input().await?;
    let runner = BridgeSolochainReceivingRunner::new(input);
    Ok(runner.start().await?)
}