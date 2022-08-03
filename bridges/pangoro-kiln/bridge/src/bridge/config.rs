use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BridgeConfig {
    pub pangoro: PangoroConfig,
    pub kiln: ChainInfoConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainInfoConfig {
    pub endpoint: String,
    pub execution_layer_endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
    pub inbound_address: String,
    pub outbound_address: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PangoroConfig {
    pub endpoint: String,
    pub contract_address: String,
    pub execution_layer_contract_address: String,
    pub private_key: String,
    pub inbound_address: String,
    pub outbound_address: String,
}