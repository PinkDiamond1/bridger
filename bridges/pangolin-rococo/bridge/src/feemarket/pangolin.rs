use bp_messages::{LaneId, MessageNonce};
use codec::Encode;
use darwinia_fee_market::types::{Order, Relayer};
use frame_support::Blake2_128Concat;
use relay_pangolin_client::PangolinChain;
use relay_substrate_client::{ChainBase, Client, TransactionSignScheme};
use relay_utils::relay_loop::Client as RelayLoopClient;
use sp_core::storage::StorageKey;
use sp_core::Pair;

use feemarket_s2s::api::FeemarketApi;
use feemarket_s2s::error::FeemarketResult;

#[derive(Clone)]
pub struct PangolinFeemarketApi {
    client: Client<PangolinChain>,
    lane_id: LaneId,
    signer: <PangolinChain as TransactionSignScheme>::AccountKeyPair,
}

impl PangolinFeemarketApi {
    pub fn new(
        client: Client<PangolinChain>,
        lane_id: LaneId,
        signer: <PangolinChain as TransactionSignScheme>::AccountKeyPair,
    ) -> Self {
        Self {
            client,
            lane_id,
            signer,
        }
    }
}

#[async_trait::async_trait]
impl FeemarketApi for PangolinFeemarketApi {
    type Chain = PangolinChain;

    async fn reconnect(&mut self) -> FeemarketResult<()> {
        Ok(self.client.reconnect().await?)
    }

    fn lane_id(&self) -> LaneId {
        self.lane_id
    }

    async fn best_finalized_header_number(
        &self,
    ) -> FeemarketResult<<Self::Chain as ChainBase>::BlockNumber> {
        Ok(self.client.best_finalized_header_number().await?)
    }

    async fn assigned_relayers(
        &self,
    ) -> FeemarketResult<
        Vec<Relayer<<Self::Chain as ChainBase>::AccountId, <Self::Chain as ChainBase>::Balance>>,
    > {
        let storage_key = StorageKey(
            feemarket_s2s::helpers::storage_prefix(
                "PangolinParachainFeeMarket".as_bytes(),
                "AssignedRelayers".as_bytes(),
            )
            .to_vec(),
        );
        Ok(self
            .client
            .storage_value(storage_key, None)
            .await?
            .unwrap_or_default())
    }

    async fn my_assigned_info(
        &self,
    ) -> FeemarketResult<
        Option<(
            usize,
            Relayer<<Self::Chain as ChainBase>::AccountId, <Self::Chain as ChainBase>::Balance>,
        )>,
    > {
        let signer_id = (*self.signer.public().as_array_ref()).into();
        let assigned_relayers = self.assigned_relayers().await?;
        let ret = assigned_relayers
            .iter()
            .position(|item| item.id == signer_id)
            // .map(|position| position as u32)
            .map(|position| {
                (
                    position,
                    assigned_relayers
                        .get(position)
                        .cloned()
                        .expect("Unreachable"),
                )
            });
        Ok(ret)
    }

    async fn order(
        &self,
        laned_id: LaneId,
        message_nonce: MessageNonce,
    ) -> FeemarketResult<
        Option<
            Order<
                <Self::Chain as ChainBase>::AccountId,
                <Self::Chain as ChainBase>::BlockNumber,
                <Self::Chain as ChainBase>::Balance,
            >,
        >,
    > {
        let storage_key = bp_runtime::storage_map_final_key::<Blake2_128Concat>(
            "PangolinParachainFeeMarket",
            "Orders",
            (laned_id, message_nonce).encode().as_slice(),
        );
        Ok(self.client.storage_value(storage_key.clone(), None).await?)
    }

    async fn relayers(&self) -> FeemarketResult<Vec<<Self::Chain as ChainBase>::AccountId>> {
        let storage_key = StorageKey(
            feemarket_s2s::helpers::storage_prefix(
                "PangolinParachainFeeMarket".as_bytes(),
                "Relayers".as_bytes(),
            )
            .to_vec(),
        );
        Ok(self
            .client
            .storage_value(storage_key, None)
            .await?
            .unwrap_or_default())
    }

    async fn relayer(
        &self,
        account: <Self::Chain as ChainBase>::AccountId,
    ) -> FeemarketResult<
        Option<Relayer<<Self::Chain as ChainBase>::AccountId, <Self::Chain as ChainBase>::Balance>>,
    > {
        let storage_key = bp_runtime::storage_map_final_key::<Blake2_128Concat>(
            "PangolinParachainFeeMarket",
            "RelayersMap",
            account.encode().as_slice(),
        );
        Ok(self.client.storage_value(storage_key.clone(), None).await?)
    }

    async fn is_relayer(&self) -> FeemarketResult<bool> {
        let signer_id = (*self.signer.public().as_array_ref()).into();
        self.relayer(signer_id).await.map(|item| item.is_some())
    }

    async fn update_relay_fee(
        &self,
        amount: <Self::Chain as ChainBase>::Balance,
    ) -> FeemarketResult<()> {
        crate::chains::pangolin::s2s_feemarket::update_relay_fee(
            &self.client,
            self.signer.clone(),
            amount,
        )
        .await
    }

    async fn update_locked_collateral(
        &self,
        amount: <Self::Chain as ChainBase>::Balance,
    ) -> FeemarketResult<()> {
        crate::chains::pangolin::s2s_feemarket::update_locked_collateral(
            &self.client,
            self.signer.clone(),
            amount,
        )
        .await
    }
}