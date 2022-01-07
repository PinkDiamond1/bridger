use lifeline::dyn_bus::DynBus;
use lifeline::{Bus, Sender};

use component_state::state::BridgeState;
use support_common::config::{Config, Names};
use support_lifeline::task::TaskStack;

use crate::bridge::PangolinRopstenBus;
use crate::bridge::PangolinRopstenConfig;
use crate::bridge::{DarwiniaEthereumMessage, EthereumScanMessage};
use crate::service::affirm::AffirmService;
use crate::service::check::CheckService;
use crate::service::extrinsics::ExtrinsicsService;
use crate::service::guard::GuardService;
use crate::service::pangolin::PangolinService;
use crate::service::redeem::RedeemService;

#[derive(Debug)]
pub struct PangolinRopstenTask {
    stack: TaskStack<PangolinRopstenBus>,
}

impl PangolinRopstenTask {
    pub fn name() -> &'static str {
        "task-pangolin-ropsten"
    }
}

impl PangolinRopstenTask {
    pub async fn new() -> color_eyre::Result<Self> {
        let state = BridgeState::new()?;
        // check config
        let _bridge_config: PangolinRopstenConfig = Config::restore(Names::BridgePangolinRopsten)?;
        let microkv = state.microkv_with_namespace(PangolinRopstenTask::name());

        crate::migrate::migrate(&microkv, 2)?;

        let bus = PangolinRopstenBus::default();
        bus.store_resource::<BridgeState>(state);

        let mut stack = TaskStack::new(bus);
        stack.spawn_service::<AffirmService>()?;
        stack.spawn_service::<CheckService>()?;
        stack.spawn_service::<RedeemService>()?;
        stack.spawn_service::<GuardService>()?;
        stack.spawn_service::<PangolinService>()?;
        stack.spawn_service::<ExtrinsicsService>()?;

        let mut tx_scan = stack.bus().tx::<DarwiniaEthereumMessage>()?;
        tx_scan
            .send(DarwiniaEthereumMessage::Scan(EthereumScanMessage::Start))
            .await?;

        Ok(Self { stack })
    }
}

impl PangolinRopstenTask {
    #[allow(dead_code)]
    pub fn stack(&self) -> &TaskStack<PangolinRopstenBus> {
        &self.stack
    }
}