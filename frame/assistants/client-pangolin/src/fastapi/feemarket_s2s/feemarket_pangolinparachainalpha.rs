use feemarket_s2s_traits::api::FeemarketApiRelay;
use feemarket_s2s_traits::error::AbstractFeemarketResult;
use feemarket_s2s_traits::types::{Chain, Order};
use support_toolkit::convert::SmartCodecMapper;

use crate::client::PangolinClient;

#[async_trait::async_trait]
impl FeemarketApiRelay for PangolinClient {
    async fn order(
        &self,
        laned_id: feemarket_s2s_traits::types::LaneId,
        message_nonce: feemarket_s2s_traits::types::MessageNonce,
    ) -> AbstractFeemarketResult<
        Option<
            Order<
                <Self::Chain as Chain>::AccountId,
                <Self::Chain as Chain>::BlockNumber,
                <Self::Chain as Chain>::Balance,
            >,
        >,
    > {
        match self
            .runtime()
            .storage()
            .pangolin_parachain_alpha_fee_market()
            .orders(laned_id, message_nonce, None)
            .await?
        {
            Some(v) => Ok(Some(SmartCodecMapper::map_to(&v)?)),
            None => Ok(None),
        }
    }
}