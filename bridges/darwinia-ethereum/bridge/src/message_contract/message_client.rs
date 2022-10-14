use std::str::FromStr;

use bridge_e2e_traits::strategy::RelayStrategy;
use futures::future;
use secp256k1::SecretKey;
use support_common::error::BridgerError;
use support_etherscan::EtherscanClient;
use web3::{
    ethabi::{encode, RawLog},
    signing::{keccak256, Key},
    transports::Http,
    types::{Address, BlockId, BlockNumber, Bytes, FilterBuilder, Proof as Web3Proof, H256, U256},
    Web3,
};

use client_contracts::{
    error::BridgeContractError,
    inbound_types::{Message, OutboundLaneData, Payload, ReceiveMessagesProof},
    outbound_types::ReceiveMessagesDeliveryProof,
    Inbound,
};
use client_contracts::{outbound_types::MessageAccepted, Outbound, SimpleFeeMarket};

use crate::{ethereum_client::types::MessagesProof, web3_helper::GasPriceOracle};

use super::{simple_fee_market::SimpleFeeMarketRelayStrategy, utils::build_eth_confirmation_proof};

pub const LANE_IDENTIFY_SLOT: u64 = 0u64;
pub const LANE_NONCE_SLOT: u64 = 1u64;
pub const LANE_MESSAGE_SLOT: u64 = 2u64;

pub struct MessageClient<T: RelayStrategy> {
    pub client: Web3<Http>,
    pub inbound: Inbound,
    pub outbound: Outbound,
    pub strategy: T,
    pub private_key: SecretKey,
    pub max_gas_price: U256,
    pub etherscan_client: EtherscanClient,
}

impl<T: RelayStrategy> GasPriceOracle for MessageClient<T> {
    fn get_web3(&self) -> &Web3<Http> {
        &self.client
    }

    fn get_etherscan_client(&self) -> Option<&EtherscanClient> {
        Some(&self.etherscan_client)
    }

    fn max_gas_price(&self) -> U256 {
        self.max_gas_price
    }
}

pub fn build_message_client_with_simple_fee_market(
    endpoint: &str,
    inbound_address: Address,
    outbound_address: Address,
    fee_market_address: Address,
    private_key: &str,
    max_gas_price: U256,
    etherscan_api_key: &str,
) -> color_eyre::Result<MessageClient<SimpleFeeMarketRelayStrategy>> {
    let transport = Http::new(endpoint)?;
    let client = Web3::new(transport);
    let inbound = Inbound::new(&client, inbound_address)?;
    let outbound = Outbound::new(&client, outbound_address)?;
    let fee_market = SimpleFeeMarket::new(&client, fee_market_address)?;
    let private_key = SecretKey::from_str(private_key)?;
    let account = (&private_key).address();
    let strategy = SimpleFeeMarketRelayStrategy::new(fee_market, account);
    let etherscan_client = EtherscanClient::new(etherscan_api_key)?;
    Ok(MessageClient {
        client,
        inbound,
        outbound,
        strategy,
        private_key,
        etherscan_client,
        max_gas_price,
    })
}

impl<T: RelayStrategy> MessageClient<T> {
    pub async fn prepare_for_messages_confirmation(
        &self,
        begin: u64,
        end: u64,
        block_number: Option<BlockNumber>,
    ) -> color_eyre::Result<ReceiveMessagesDeliveryProof> {
        let at_block = block_number.map(BlockId::Number);
        let inbound_lane_data = self.inbound.data(at_block).await?;
        let messages_proof =
            build_eth_confirmation_proof(&self.client, &self.inbound, begin, end, block_number)
                .await?;
        Ok(ReceiveMessagesDeliveryProof {
            inbound_lane_data,
            messages_proof,
        })
    }

    pub async fn prepare_for_messages_delivery(
        &self,
        begin: u64,
        end: u64,
        block_number: Option<BlockNumber>,
    ) -> color_eyre::Result<ReceiveMessagesProof> {
        let outbound_lane_data = self
            .build_messages_data(begin, end, block_number.map(BlockId::from))
            .await?;
        let proof = self
            .build_messages_proof(begin, end, block_number)
            .await?
            .get_token()?;
        let messages_proof = Bytes(encode(&[proof]));
        Ok(ReceiveMessagesProof {
            outbound_lane_data,
            messages_proof,
        })
    }

    pub async fn build_messages_data(
        &self,
        begin: u64,
        end: u64,
        at_block: Option<BlockId>,
    ) -> color_eyre::Result<OutboundLaneData> {
        let outbound_data = self.outbound.data(at_block).await?;
        let outbound_lane_nonce = self.outbound.outbound_lane_nonce(at_block).await?;
        let (outbound_begin, _outbound_end) = (
            outbound_lane_nonce.latest_received_nonce + 1,
            outbound_lane_nonce.latest_generated_nonce,
        );
        let messages = Vec::from_iter(
            outbound_data.messages
                [(begin - outbound_begin) as usize..=(end - outbound_begin) as usize]
                .iter()
                .cloned(),
        );

        if (end - begin + 1) as usize != messages.len() {
            return Err(BridgerError::Custom("Build messages data failed".into()).into());
        }

        let accepted_events = self.query_message_accepted_events(begin, end).await?;
        let messages: Vec<Message> = std::iter::zip(messages, accepted_events)
            .into_iter()
            .map(|(message, event)| Message {
                encoded_key: message.encoded_key,
                payload: Payload {
                    source: event.source,
                    target: event.target,
                    encoded: event.encoded,
                },
            })
            .collect();

        Ok(OutboundLaneData {
            latest_received_nonce: outbound_data.latest_received_nonce,
            messages,
        })
    }

    pub async fn query_message_accepted_events(
        &self,
        begin: u64,
        end: u64,
    ) -> color_eyre::Result<Vec<MessageAccepted>> {
        let logs: Result<Vec<Option<MessageAccepted>>, _> = future::try_join_all(
            (begin..=end).map(|nonce| self.query_message_accepted_with_retry(nonce)),
        )
        .await;
        if let Some(logs) = logs?.into_iter().collect::<Option<Vec<_>>>() {
            Ok(logs)
        } else {
            Err(BridgerError::Custom(format!(
                "Failed to get message events from {:?} to {:?}",
                begin, end
            ))
            .into())
        }
    }

    pub async fn query_message_accepted_with_retry(
        &self,
        nonce: u64,
    ) -> color_eyre::Result<Option<MessageAccepted>> {
        let mut count = 0;
        while count < 3 {
            match self.query_message_accepted(nonce).await {
                Ok(v) => return Ok(v),
                Err(error) => {
                    count += 1;
                    if count > 3 {
                        return Err(error);
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(count)).await;
                }
            }
        }
        Ok(None)
    }

    pub async fn query_message_accepted(
        &self,
        nonce: u64,
    ) -> color_eyre::Result<Option<MessageAccepted>> {
        let event = self.outbound.contract.abi().event("MessageAccepted")?;
        let mut filter = FilterBuilder::default();
        filter = filter.from_block(BlockNumber::Earliest);
        filter = filter.address(vec![self.outbound.contract.address()]);
        filter = filter.topics(
            Some(vec![event.signature()]),
            Some(vec![H256::from_low_u64_be(nonce)]),
            None,
            None,
        );
        let logs = self.client.eth().logs(filter.build()).await?;

        let events: Vec<MessageAccepted> = logs
            .into_iter()
            .map(|l| {
                let row_log = RawLog {
                    topics: l.topics.clone(),
                    data: l.data.0.clone(),
                };
                let block_number = l
                    .block_number
                    .ok_or_else(|| BridgeContractError::Custom("Failed toget block number".into()))?
                    .as_u64();
                MessageAccepted::from_log(event.parse_log(row_log)?, block_number)
            })
            .collect::<Result<Vec<MessageAccepted>, BridgeContractError>>()?;
        match events.as_slice() {
            [x] => Ok(Some(x.clone())),
            _ => Ok(None),
        }
    }

    pub async fn build_messages_proof(
        &self,
        begin: u64,
        end: u64,
        block_number: Option<BlockNumber>,
    ) -> color_eyre::Result<MessagesProof> {
        let lane_id_proof = self
            .get_storage_proof_with_retry(
                self.outbound.contract.address(),
                vec![U256::from(LANE_IDENTIFY_SLOT)],
                block_number,
            )
            .await?
            .ok_or_else(|| BridgerError::Custom("Failed to get lane_id_proof".into()))?;
        let lane_nonce_proof = self
            .get_storage_proof_with_retry(
                self.outbound.contract.address(),
                vec![U256::from(LANE_NONCE_SLOT)],
                block_number,
            )
            .await?
            .ok_or_else(|| BridgerError::Custom("Failed to get lane_nonce_proof".into()))?;
        let message_keys = Self::build_message_storage_keys(begin, end);
        let message_proof = self
            .get_storage_proof_with_retry(
                self.outbound.contract.address(),
                message_keys,
                block_number,
            )
            .await?
            .ok_or_else(|| BridgerError::Custom("Failed to get message_proof".into()))?;

        let account_proof = Self::encode_proof(&lane_id_proof.account_proof);
        let lane_id_proof = Self::encode_proof(&lane_id_proof.storage_proof[0].proof);
        let lane_nonce_proof = Self::encode_proof(&lane_nonce_proof.storage_proof[0].proof);
        let lane_messages_proof = message_proof
            .storage_proof
            .iter()
            .map(|x| Self::encode_proof(&x.proof))
            .collect::<Vec<Bytes>>();

        Ok(MessagesProof {
            account_proof,
            lane_id_proof,
            lane_nonce_proof,
            lane_messages_proof,
        })
    }

    fn encode_proof(proofs: &[Bytes]) -> Bytes {
        Bytes::from(
            &rlp::encode_list::<Vec<u8>, _>(
                proofs
                    .iter()
                    .map(|x| x.0.clone())
                    .collect::<Vec<Vec<u8>>>()
                    .as_slice(),
            )[..],
        )
    }

    pub fn build_message_storage_keys(begin: u64, end: u64) -> Vec<U256> {
        (begin..=end)
            .map(|pos| {
                let pos = U256::from(pos);
                let slot = U256::from(LANE_MESSAGE_SLOT);
                let bytes: &mut [u8] = &mut [0u8; 64];
                pos.to_big_endian(&mut bytes[..32]);
                slot.to_big_endian(&mut bytes[32..]);
                U256::from(keccak256(bytes))
            })
            .collect()
    }

    pub async fn get_storage_proof_with_retry(
        &self,
        address: Address,
        storage_keys: Vec<U256>,
        block_number: Option<BlockNumber>,
    ) -> color_eyre::Result<Option<Web3Proof>> {
        let mut count = 0;
        while count < 3 {
            match self
                .get_storage_proof(address, storage_keys.clone(), block_number)
                .await
            {
                Ok(v) => return Ok(v),
                Err(error) => {
                    count += 1;
                    if count > 3 {
                        return Err(error);
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(count)).await;
                }
            }
        }
        Ok(None)
    }

    pub async fn get_storage_proof(
        &self,
        address: Address,
        storage_keys: Vec<U256>,
        block_number: Option<BlockNumber>,
    ) -> color_eyre::Result<Option<Web3Proof>> {
        Ok(self
            .client
            .eth()
            .proof(address, storage_keys, block_number)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use secp256k1::SecretKey;
    use web3::{
        contract::Options,
        types::{Address, U64},
    };

    use super::*;

    fn test_client() -> MessageClient<SimpleFeeMarketRelayStrategy> {
        let client = build_message_client_with_simple_fee_market(
            "http://localhost:8545",
            Address::from_str("0x588abe3F7EE935137102C5e2B8042788935f4CB0").unwrap(),
            Address::from_str("0xee4f69fc69F2C203a0572e43375f68a6e9027998").unwrap(),
            Address::from_str("0x721F10bdE716FF44F596Afa2E8726aF197e6218E").unwrap(),
            "",
            U256::from_dec_str("1000000").unwrap(),
            "",
        )
        .unwrap();
        client
    }

    fn test_darwinia_client() -> MessageClient<SimpleFeeMarketRelayStrategy> {
        build_message_client_with_simple_fee_market(
            "https://darwinia-rpc.darwinia.network",
            Address::from_str("0x3E37361F50a178e05E5d81234dDE67E6cC991ed1").unwrap(),
            Address::from_str("0x634370aCf53cf55ad270E084442ea7A23B43B26a").unwrap(),
            Address::from_str("0xB59a893f5115c1Ca737E36365302550074C32023").unwrap(),
            "",
            U256::from_dec_str("1000000").unwrap(),
            "",
        )
        .unwrap()
    }

    #[ignore]
    #[tokio::test]
    async fn test_get_storage_proof() {
        let client = test_client();
        let (begin, end) = (1, 2);
        let message_keys =
            MessageClient::<SimpleFeeMarketRelayStrategy>::build_message_storage_keys(begin, end);
        println!("Message keys: {:?}", message_keys);
        let message_proof = client
            .get_storage_proof(client.outbound.contract.address(), message_keys, None)
            .await
            .unwrap()
            .ok_or_else(|| BridgerError::Custom("Failed to get message_proof".into()))
            .unwrap();
        println!("Proof: {:?}", message_proof);
    }

    #[ignore]
    #[tokio::test]
    async fn test_build_lane_data() {
        let client = test_darwinia_client();
        let outbound_lane_nonce = client.outbound.outbound_lane_nonce(None).await.unwrap();
        let (begin, end) = (
            outbound_lane_nonce.latest_received_nonce + 1,
            outbound_lane_nonce.latest_generated_nonce,
        );
        let lane_data = client.build_messages_data(begin, end, None).await.unwrap();
        println!("Lane data: {:?}", lane_data);
    }

    #[ignore]
    #[tokio::test]
    async fn test_query_message_accepted_events() {
        let client = test_client();
        let logs = client.query_message_accepted_events(1, 2).await.unwrap();
        println!("Logs: {:?}", logs);
    }

    #[ignore]
    #[tokio::test]
    async fn test_query_message_event() {
        let client = test_darwinia_client();
        let event = client.query_message_accepted(2).await.unwrap();
        println!("event: {:?}", event);
    }

    #[ignore]
    #[tokio::test]
    async fn test_receive_messages_proof() {
        let ethereum_client = test_client();
        let darwinia_client = test_darwinia_client();
        let private_key = SecretKey::from_str("//Alice").unwrap();
        let proof = ethereum_client
            .prepare_for_messages_delivery(1, 2, Some(BlockNumber::Number(U64::from(1580730u64))))
            .await
            .unwrap();
        println!("proof: {:?}", proof);
        let tx = darwinia_client
            .inbound
            .receive_messages_proof(proof, U256::from(2), &private_key, Options::default())
            .await
            .unwrap();
        println!("tx: {:?}", tx);

        let inbound_status = darwinia_client.inbound.data(None).await.unwrap();
        println!("darwinia inbound: {:?}", inbound_status);
    }
}