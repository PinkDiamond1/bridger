use bp_messages::{LaneId, MessageNonce};
use codec::Encode;
use dp_fee::{Order, Relayer};
use drml_common_primitives::AccountId;
use drml_common_primitives::Balance;
use drml_common_primitives::BlockNumber;
use frame_support::Blake2_128Concat;
use relay_substrate_client::{
    ChainBase, Client, SignParam, TransactionSignScheme, UnsignedTransaction,
};
use relay_utils::relay_loop::Client as RelayLoopClient;
use sp_core::storage::StorageKey;
use sp_core::{Bytes, Pair};

use crate::{patch, PangoroChain};

#[derive(Clone)]
pub struct PangoroApi {
    client: Client<PangoroChain>,
}

impl PangoroApi {
    pub fn new(client: Client<PangoroChain>) -> Self {
        Self { client }
    }
}

impl PangoroApi {
    pub async fn reconnect(&mut self) -> color_eyre::Result<()> {
        Ok(self.client.reconnect().await?)
    }

    /// Query assigned relayers
    pub async fn assigned_relayers(&self) -> color_eyre::Result<Vec<Relayer<AccountId, Balance>>> {
        let storage_key = StorageKey(
            patch::storage_prefix("FeeMarket".as_bytes(), "AssignedRelayers".as_bytes()).to_vec(),
        );
        Ok(self
            .client
            .storage_value(storage_key, None)
            .await?
            .unwrap_or_default())
    }

    /// Query order
    pub async fn order(
        &self,
        laned_id: LaneId,
        message_nonce: MessageNonce,
    ) -> color_eyre::Result<Option<Order<AccountId, BlockNumber, Balance>>> {
        let storage_key = bp_runtime::storage_map_final_key::<Blake2_128Concat>(
            "FeeMarket",
            "Orders",
            (laned_id, message_nonce).encode().as_slice(),
        );
        Ok(self.client.storage_value(storage_key.clone(), None).await?)
    }

    /// Query all relayers
    pub async fn relayers(&self) -> color_eyre::Result<Vec<AccountId>> {
        let storage_key = StorageKey(
            patch::storage_prefix("FeeMarket".as_bytes(), "Relayers".as_bytes()).to_vec(),
        );
        Ok(self
            .client
            .storage_value(storage_key, None)
            .await?
            .unwrap_or_default())
    }

    /// Query relayer info by account id
    pub async fn relayer(
        &self,
        account: AccountId,
    ) -> color_eyre::Result<Option<Relayer<AccountId, Balance>>> {
        let storage_key = bp_runtime::storage_map_final_key::<Blake2_128Concat>(
            "FeeMarket",
            "RelayersMap",
            account.encode().as_slice(),
        );
        Ok(self.client.storage_value(storage_key.clone(), None).await?)
    }

    pub async fn is_relayer(&self, account: AccountId) -> color_eyre::Result<bool> {
        self.relayer(account).await.map(|item| item.is_some())
    }

    /// Return number of the best finalized block.
    pub async fn best_finalized_header_number(
        &self,
    ) -> color_eyre::Result<drml_common_primitives::BlockNumber> {
        Ok(self.client.best_finalized_header_number().await?)
    }

    /// Update relay fee
    pub async fn update_relay_fee(
        &self,
        signer: <PangoroChain as TransactionSignScheme>::AccountKeyPair,
        amount: <PangoroChain as ChainBase>::Balance,
    ) -> color_eyre::Result<()> {
        let signer_id = (*signer.public().as_array_ref()).into();
        let genesis_hash = *self.client.genesis_hash();
        let (spec_version, transaction_version) = self.client.simple_runtime_version().await?;
        self.client
            .submit_signed_extrinsic(signer_id, move |_, transaction_nonce| {
                Bytes(
                    PangoroChain::sign_transaction(SignParam {
                        spec_version,
                        transaction_version,
                        genesis_hash,
                        signer,
                        era: relay_substrate_client::TransactionEra::immortal(),
                        unsigned: UnsignedTransaction::new(
                            pangoro_runtime::FeeMarketCall::update_relay_fee { new_fee: amount }
                                .into(),
                            transaction_nonce,
                        ),
                    })
                    .encode(),
                )
            })
            .await?;
        Ok(())
    }

    /// Update locked collateral
    pub async fn update_locked_collateral(
        &mut self,
        signer: <PangoroChain as TransactionSignScheme>::AccountKeyPair,
        amount: <PangoroChain as ChainBase>::Balance,
    ) -> color_eyre::Result<()> {
        let signer_id = (*signer.public().as_array_ref()).into();
        let genesis_hash = *self.client.genesis_hash();
        let (spec_version, transaction_version) = self.client.simple_runtime_version().await?;
        self.client
            .submit_signed_extrinsic(signer_id, move |_, transaction_nonce| {
                Bytes(
                    PangoroChain::sign_transaction(SignParam {
                        spec_version,
                        transaction_version,
                        genesis_hash,
                        signer,
                        era: relay_substrate_client::TransactionEra::immortal(),
                        unsigned: UnsignedTransaction::new(
                            pangoro_runtime::FeeMarketCall::update_locked_collateral {
                                new_collateral: amount,
                            }
                            .into(),
                            transaction_nonce,
                        ),
                    })
                    .encode(),
                )
            })
            .await?;
        Ok(())
    }
}
