use crate::service::ecdsa_relay::types::EcdsaSource;

pub struct CollectingNewMessageRootSignaturesRunner {
    source: EcdsaSource,
}

impl CollectingNewMessageRootSignaturesRunner {
    pub fn new(source: EcdsaSource) -> Self {
        Self { source }
    }
}

impl CollectingNewMessageRootSignaturesRunner {
    pub async fn start(&self) -> color_eyre::Result<Option<u32>> {
        let client_pangoro_substrate = &self.source.client_pangoro_substrate;
        let subquery = &self.source.subquery;
        let from_block = self.source.block.unwrap_or_default();
        let pangoro_evm_account = &self.source.pangoro_evm_account;

        let cacse = subquery
            .next_collecting_new_message_root_signatures_event(from_block)
            .await?;
        if cacse.is_none() {
            tracing::debug!(
                target: "pangoro-goerli",
                "[pangoro] [ecdsa] no more new message root signatures events after {}",
                from_block,
            );
            return Ok(None);
        }
        let event = cacse.expect("Unreachable");
        tracing::info!(
            target: "pangoro-goerli",
            "[pangoro] [ecdsa] found new message root signature event from block {}",
            event.block_number,
        );
        if !client_pangoro_substrate
            .is_ecdsa_authority(Some(event.block_number), &pangoro_evm_account.address()?.0)
            .await?
        {
            tracing::warn!(
                target: "pangoro-goerli",
                "[pangoro] [ecdsa] you are not authority account. nothing to do.",
            );
            return Ok(Some(event.block_number));
        }

        let address = pangoro_evm_account.address()?;
        let signature = pangoro_evm_account.sign(event.message.as_slice())?;
        let hash = client_pangoro_substrate
            .submit_new_message_root_signature(address.0, signature)
            .await?;

        tracing::info!(
            target: "pangoro-goerli",
            "[pangoro] [ecdsa] submitted new message root signature: {}",
            array_bytes::bytes2hex("0x", &hash.0),
        );
        Ok(Some(event.block_number))
    }
}