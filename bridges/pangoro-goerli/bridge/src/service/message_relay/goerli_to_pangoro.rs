use std::str::FromStr;
use std::time::Duration;

use bridge_e2e_traits::strategy::RelayStrategy;
use client_contracts::PosaLightClient;
use web3::types::{Address, BlockId, BlockNumber, H256, U256};

use crate::message_contract::darwinia_message_client::{
    build_darwinia_message_client, DarwiniaMessageClient,
};
use crate::message_contract::fee_market::FeeMarketRelayStrategy;
use crate::message_contract::message_client::build_message_client_with_simple_fee_market;
use crate::message_contract::simple_fee_market::SimpleFeeMarketRelayStrategy;
use crate::web3_helper::wait_for_transaction_confirmation;
use crate::{
    goerli_client::client::GoerliClient, message_contract::message_client::MessageClient,
    pangoro_client::client::PangoroClient,
};

use crate::bridge::{BridgeBus, BridgeConfig};
use lifeline::{Lifeline, Service, Task};
use support_common::config::{Config, Names};
use support_lifeline::service::BridgeService;

#[derive(Debug)]
pub struct GoerliPangoroMessageRelay {
    _greet_delivery: Lifeline,
    _greet_confirmation: Lifeline,
}

impl BridgeService for GoerliPangoroMessageRelay {}

impl Service for GoerliPangoroMessageRelay {
    type Bus = BridgeBus;
    type Lifeline = color_eyre::Result<Self>;

    fn spawn(_bus: &Self::Bus) -> Self::Lifeline {
        let _greet_delivery = Self::try_task("message-relay-goerli-to-pangoro", async move {
            while let Err(error) = start_delivery().await {
                tracing::error!(
                    target: "pangoro-goerli",
                    "Failed to start goerli-to-pangoro message relay service, restart after some seconds: {:?}",
                    error
                );
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            }
            Ok(())
        });
        let _greet_confirmation = Self::try_task(
            "message-confirmation-pangoro-to-goerli",
            async move {
                while let Err(error) = start_confirmation().await {
                    tracing::error!(
                        target: "pangoro-goerli",
                        "Failed to start goerli-to-pangoro message confirmation service, restart after some seconds: {:?}",
                        error
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                }
                Ok(())
            },
        );
        Ok(Self {
            _greet_delivery,
            _greet_confirmation,
        })
    }
}

async fn message_relay_client_builder(
) -> color_eyre::Result<MessageRelay<SimpleFeeMarketRelayStrategy, FeeMarketRelayStrategy>> {
    let config: BridgeConfig = Config::restore(Names::BridgePangoroGoerli)?;
    let beacon_light_client = PangoroClient::new(
        &config.pangoro_evm.endpoint,
        &config.pangoro_evm.contract_address,
        &config.pangoro_evm.execution_layer_contract_address,
        &config.pangoro_evm.private_key,
        config.pangoro_evm.gas_option(),
    )?;
    let beacon_rpc_client = GoerliClient::new(&config.goerli.endpoint)?;
    let source = build_message_client_with_simple_fee_market(
        &config.goerli.execution_layer_endpoint,
        Address::from_str(&config.goerli.inbound_address)?,
        Address::from_str(&config.goerli.outbound_address)?,
        Address::from_str(&config.goerli.fee_market_address)?,
        Address::from_str(&config.goerli.account)?,
        Some(&config.goerli.private_key),
        config.goerli.gas_option(),
    )
    .unwrap();
    let target = build_darwinia_message_client(
        &config.pangoro_evm.endpoint,
        Address::from_str(&config.pangoro_evm.inbound_address)?,
        Address::from_str(&config.pangoro_evm.outbound_address)?,
        Address::from_str(&config.pangoro_evm.chain_message_committer_address)?,
        Address::from_str(&config.pangoro_evm.lane_message_committer_address)?,
        Address::from_str(&config.pangoro_evm.fee_market_address)?,
        Address::from_str(&config.pangoro_evm.account)?,
        Some(&config.pangoro_evm.private_key),
        config.index.to_pangoro_thegraph()?,
        config.pangoro_evm.gas_option(),
    )
    .unwrap();
    let posa_light_client = PosaLightClient::new(
        source.client.clone(),
        Address::from_str(&config.goerli.posa_light_client_address)?,
    )?;
    Ok(MessageRelay {
        source,
        target,
        posa_light_client,
        beacon_rpc_client,
        beacon_light_client,
    })
}

async fn start_delivery() -> color_eyre::Result<()> {
    let mut message_relay_service = message_relay_client_builder().await?;
    loop {
        if let Err(error) = message_relay_service.message_relay().await {
            tracing::error!(
                target: "pangoro-goerli",
                "[MessageDelivery][goerli=>Pangoro] Failed to relay message: {:?}",
                error
            );
            return Err(error);
        }
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
    }
}

async fn start_confirmation() -> color_eyre::Result<()> {
    let message_relay_service = message_relay_client_builder().await?;
    loop {
        if let Err(error) = message_relay_service.message_confirm().await {
            tracing::error!(
                target: "pangoro-goerli",
                "[MessageConfirmation][goerli=>Pangoro] Failed to confirm message: {:?}",
                error
            );
            return Err(error);
        }
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
    }
}

pub struct MessageRelay<S0: RelayStrategy, S1: RelayStrategy> {
    pub source: MessageClient<S0>,
    pub target: DarwiniaMessageClient<S1>,
    pub posa_light_client: PosaLightClient,
    pub beacon_rpc_client: GoerliClient,
    pub beacon_light_client: PangoroClient,
}

impl<S0: RelayStrategy, S1: RelayStrategy> MessageRelay<S0, S1> {
    async fn message_relay(&mut self) -> color_eyre::Result<()> {
        let received_nonce = self.target.inbound.inbound_lane_nonce(None).await?;
        let latest_nonce = self.source.outbound.outbound_lane_nonce(None).await?;

        if received_nonce.last_delivered_nonce == latest_nonce.latest_generated_nonce {
            tracing::info!(
                target: "pangoro-goerli",
                "[MessageDelivery][Goerli=>Pangoro] Last delivered nonce is {:?}, equal to lastest generated. Do nothing.",
                received_nonce.last_delivered_nonce,
            );
            return Ok(());
        }

        let finalized_block_number = match self.best_source_block_at_target().await? {
            None => {
                tracing::info!(
                    target: "pangoro-goerli",
                    "[MessageDelivery][Goerli=>Pangoro] Wait for execution layer relay",
                );
                return Ok(());
            }
            Some(num) => num,
        };

        let outbound_nonce = self
            .source
            .outbound
            .outbound_lane_nonce(Some(BlockId::Number(BlockNumber::from(
                finalized_block_number,
            ))))
            .await?;

        let (begin, end) = (
            latest_nonce.latest_received_nonce + 1,
            latest_nonce.latest_generated_nonce,
        );

        if received_nonce.last_delivered_nonce >= outbound_nonce.latest_generated_nonce {
            tracing::info!(
                target: "pangoro-goerli",
                "[MessageDelivery][Goerli=>Pangoro] Messages: [{:?}, {:?}] need to be relayed, wait for header relay",
                begin,
                end
            );
            return Ok(());
        }

        let (begin, end) = (
            outbound_nonce.latest_received_nonce + 1,
            outbound_nonce.latest_generated_nonce,
        );

        tracing::info!(
            target: "pangoro-goerli",
            "[MessageDelivery][Goerli=>Pangoro] Try to relay messages: [{:?}, {:?}]",
            begin,
            end
        );

        let proof = self
            .source
            .prepare_for_messages_delivery(
                begin,
                end,
                Some(BlockNumber::from(finalized_block_number)),
            )
            .await?;
        let encoded_keys: Vec<U256> = proof
            .outbound_lane_data
            .messages
            .iter()
            .map(|x| x.encoded_key)
            .collect();

        let max_unconfirmed_messages = 20;

        // Calculate devliery_size parameter in inbound.receive_messages_proof
        let mut count = 0;
        for (index, key) in encoded_keys.iter().enumerate() {
            let current = index as u64 + begin;

            // Messages less or equal than last_delivered_nonce have been delivered.
            let is_delivered = current <= received_nonce.last_delivered_nonce;
            let not_beyond_confirm =
                current - received_nonce.last_confirmed_nonce <= max_unconfirmed_messages;

            if not_beyond_confirm && (is_delivered || self.source.strategy.decide(*key).await?) {
                count += 1;
            } else {
                break;
            }
        }
        if count == 0 {
            tracing::info!(
                target: "pangoro-goerli",
                "[MessageDelivery][Goerli=>Pangoro] Decided not to relay",
            );
            return Ok(());
        }
        tracing::info!(
            target: "pangoro-goerli",
            "[MessageDelivery][Goerli=>Pangoro] Relaying messages: [{:?}, {:?}]",
            begin,
            begin + count - 1,
        );

        let tx = self
            .target
            .inbound
            .receive_messages_proof(
                proof,
                U256::from(count),
                &self.target.private_key()?,
                self.target.gas_option.clone(),
            )
            .await?;

        tracing::info!(
            target: "pangoro-goerli",
            "[MessageDelivery][Goerli=>Pangoro] Sending tx: {:?}",
            tx
        );

        wait_for_transaction_confirmation(
            tx,
            self.target.client.transport(),
            Duration::from_secs(5),
            1,
        )
        .await?;

        Ok(())
    }

    async fn best_source_block_at_target(&self) -> color_eyre::Result<Option<u64>> {
        let finalized = self
            .beacon_light_client
            .beacon_light_client
            .finalized_header()
            .await?;
        let block = self
            .beacon_rpc_client
            .get_beacon_block(finalized.slot)
            .await?;
        let execution_state_root = self
            .beacon_light_client
            .execution_layer_state_root(None)
            .await?;
        if execution_state_root != H256::from_str(&block.body.execution_payload.state_root)? {
            Ok(None)
        } else {
            Ok(Some(block.body.execution_payload.block_number.parse()?))
        }
    }
}

impl<S0: RelayStrategy, S1: RelayStrategy> MessageRelay<S0, S1> {
    pub async fn message_confirm(&self) -> color_eyre::Result<()> {
        let source_outbound_lane_data = self.source.outbound.outbound_lane_nonce(None).await?;
        if source_outbound_lane_data.latest_received_nonce
            == source_outbound_lane_data.latest_generated_nonce
        {
            tracing::info!(
                target: "pangoro-goerli",
                "[MessageConfirmation][Goerli=>Pangoro] All confirmed({:?}), nothing to do.",
                source_outbound_lane_data
            );
            return Ok(());
        }

        // query last relayed header
        let last_relayed_target_block_in_source = self.best_target_block_at_source().await?;

        // assemble unrewarded relayers state
        let target_inbound_state = self
            .target
            .inbound
            .inbound_lane_nonce(Some(BlockId::from(BlockNumber::from(
                last_relayed_target_block_in_source,
            ))))
            .await?;
        let (begin, end) = (
            target_inbound_state.relayer_range_front,
            target_inbound_state.relayer_range_back,
        );
        if source_outbound_lane_data.latest_received_nonce
            == target_inbound_state.last_delivered_nonce
        {
            tracing::info!(
                target: "pangoro-goerli",
                "[MessageConfirmation][Goerli=>Pangoro] Nonce {:?} was confirmed, wait for delivery from {:?} to {:?}. ",
                source_outbound_lane_data.latest_received_nonce,
                target_inbound_state.last_delivered_nonce + 1,
                source_outbound_lane_data.latest_generated_nonce
            );
            return Ok(());
        }

        tracing::info!(
            target: "pangoro-goerli",
            "[MessageConfirmation][Goerli=>Pangoro] Try to confirm nonces [{:?}:{:?}]",
            begin,
            end,
        );
        // read proof
        let proof = self
            .target
            .prepare_for_messages_confirmation(Some(BlockId::Number(BlockNumber::from(
                last_relayed_target_block_in_source,
            ))))
            .await?;

        // send proof
        let hash = self
            .source
            .outbound
            .receive_messages_delivery_proof(
                proof,
                &self.source.private_key()?,
                self.source.gas_option.clone(),
            )
            .await?;

        tracing::info!(
            target: "relay-s2s",
            "[MessageConfirmation][Goerli=>Pangoro] Messages confirmation tx: {:?}",
            hash
        );
        wait_for_transaction_confirmation(
            hash,
            self.source.client.transport(),
            Duration::from_secs(5),
            1,
        )
        .await?;

        Ok(())
    }

    async fn best_target_block_at_source(&self) -> color_eyre::Result<u64> {
        Ok(self.posa_light_client.block_number().await?.as_u64())
    }
}