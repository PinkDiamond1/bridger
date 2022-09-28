use client_darwinia::client::DarwiniaClient;
use client_darwinia::component::DarwiniaClientComponent;
use client_darwinia::types::runtime_types::darwinia_bridge_ethereum::EthereumRelayHeaderParcel;
use lifeline::Sender;
use microkv::namespace::NamespaceMicroKV;
use postage::broadcast;

use shadow_liketh::component::ShadowComponent;
use shadow_liketh::shadow::Shadow;
use shadow_liketh::types::BridgeName;
use support_common::config::{Config, Names};

use crate::bridge::{DarwiniaEthereumConfig, Extrinsic, ToExtrinsicsMessage};

pub struct AffirmHandler {
    microkv: NamespaceMicroKV,
    sender_to_extrinsics: broadcast::Sender<ToExtrinsicsMessage>,
    client: DarwiniaClient,
    shadow: Shadow,
}

impl AffirmHandler {
    pub async fn new(
        microkv: NamespaceMicroKV,
        sender_to_extrinsics: broadcast::Sender<ToExtrinsicsMessage>,
    ) -> Self {
        loop {
            match AffirmHandler::build(microkv.clone(), sender_to_extrinsics.clone()).await {
                Ok(handler) => return handler,
                Err(err) => {
                    tracing::error!(
                        target: "darwinia-ethereum",
                        "[ethereum] [affirm] [handle] Failed to init affirm handler. err: {:#?}",
                        err
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                }
            }
        }
    }

    async fn build(
        microkv: NamespaceMicroKV,
        sender_to_extrinsics: broadcast::Sender<ToExtrinsicsMessage>,
    ) -> color_eyre::Result<Self> {
        tracing::info!(target: "darwinia-ethereum", chain = "ethereum", action = "affirm", "SERVICE RESTARTING...");
        let bridge_config: DarwiniaEthereumConfig = Config::restore(Names::BridgeDarwiniaEthereum)?;

        // Subxt client
        let client = DarwiniaClientComponent::component(bridge_config.darwinia).await?;

        // Shadow client
        let shadow = ShadowComponent::component(
            bridge_config.shadow,
            bridge_config.ethereum,
            bridge_config.web3,
            BridgeName::DarwiniaEthereum,
        )?;

        tracing::info!(
            target: "darwinia-ethereum", chain = "ethereum", action = "affirm",
            "[ethereum] [affirm] [handle] ✨ SERVICE STARTED: ETHEREUM <> DARWINIA RELAY"
        );
        Ok(AffirmHandler {
            microkv,
            sender_to_extrinsics,
            client,
            shadow,
        })
    }
}

impl AffirmHandler {
    pub async fn affirm(&mut self) -> color_eyre::Result<()> {
        let last_confirmed = self.client.ethereum().last_confirmed().await?;
        let mut relayed = self.microkv.get_as("affirm.relayed")?.unwrap_or(0);
        let target = self.microkv.get_as("affirm.target")?.unwrap_or(0);

        tracing::info!(
            target: "darwinia-ethereum",
            "[ethereum] [affirm] [handle] The last confirmed ethereum block is {}",
            last_confirmed
        );

        if last_confirmed > relayed {
            self.microkv.put("affirm.relayed", &last_confirmed)?;
            relayed = last_confirmed;
        } else {
            tracing::trace!(
                target: "darwinia-ethereum",
                "[ethereum] [affirm] [handle] The last relayed ethereum block is {}",
                relayed
            );
        }

        if target > relayed {
            tracing::trace!(
                target: "darwinia-ethereum",
                "[ethereum] [affirm] [handle] You are affirming ethereum block {}",
                target
            );
            self.do_affirm(target).await?
        } else {
            tracing::trace!(
                target: "darwinia-ethereum",
                "[ethereum] [affirm] [handle] The block ({}) is less or equal with relayed {}, so nothing to do",
                target,
                relayed,
            );
        }

        Ok(())
    }

    pub fn update_target(&self, block_number: u64) -> color_eyre::Result<()> {
        let target = self.microkv.get_as("affirm.target")?.unwrap_or(0);

        if block_number > target {
            self.microkv.put("affirm.target", &block_number)?;
        }

        Ok(())
    }

    async fn do_affirm(&mut self, target: u64) -> color_eyre::Result<()> {
        // /////////////////////////
        // checking before affirm
        // /////////////////////////
        // 1. pendings check
        let pending_headers = self
            .client
            .runtime()
            .storage()
            .ethereum_relay()
            .pending_relay_header_parcels(None)
            .await?;
        for pending_header in pending_headers {
            let pending_block_number = pending_header.1.header.number as u64;
            if pending_block_number >= target {
                tracing::trace!(
                    target: "darwinia-ethereum",
                    "[ethereum] [affirm] [handle] The affirming target block {} is in pending",
                    target
                );
                return Ok(());
            }
        }

        // 2. affirmations check
        for (_game_id, game) in self.client.ethereum().affirmations().await?.iter() {
            for (_round_id, affirmations) in game.iter() {
                if client_darwinia::helpers::affirmations_contains_block(affirmations, target) {
                    tracing::trace!(
                        target: "darwinia-ethereum",
                        "[ethereum] [affirm] [handle] The affirming target block {} is in the relayer game",
                        target
                    );
                    return Ok(());
                }
            }
        }

        tracing::trace!(
            target: "darwinia-ethereum",
            "[ethereum] [affirm] [handle] Prepare to affirm ethereum block: {}",
            target
        );

        match self.shadow.parcel(target).await {
            Ok(parcel) => {
                let parcel: EthereumRelayHeaderParcel = parcel.try_into()?;
                if parcel.parent_mmr_root.to_fixed_bytes() == [0u8; 32] {
                    tracing::trace!(
                        target: "darwinia-ethereum",
                        "[ethereum] [affirm] [handle] Shadow service failed to provide parcel for block {}",
                        target
                    );
                    return Ok(());
                }

                // /////////////////////////
                // do affirm
                // /////////////////////////
                let ex = Extrinsic::Affirm(parcel);
                self.sender_to_extrinsics
                    .send(ToExtrinsicsMessage::Extrinsic(ex))
                    .await?
            }
            Err(err) => {
                return Err(err.into());
            }
        }

        Ok(())
    }
}