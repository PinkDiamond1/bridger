use support_lifeline::task::TaskStack;

use crate::bridge::BridgeBus;
use crate::service::header::{
    PangolinToParachainHeaderRelayService, RococoToPangolinHeaderRelayService,
    RococoToPangolinParaHeaderRelayService,
};
use crate::service::message::MessageRelayService;
use crate::service::subscribe::SubscribeService;

#[derive(Debug)]
pub struct BridgeTask {
    stack: TaskStack<BridgeBus>,
}

impl BridgeTask {
    pub fn name() -> &'static str {
        "task-pangolin-pangolinparachain"
    }
}

impl BridgeTask {
    pub async fn new() -> color_eyre::Result<Self> {
        let bus = BridgeBus::default();

        let mut stack = TaskStack::new(bus);
        stack.spawn_service::<PangolinToParachainHeaderRelayService>()?;
        stack.spawn_service::<RococoToPangolinHeaderRelayService>()?;
        stack.spawn_service::<RococoToPangolinParaHeaderRelayService>()?;
        stack.spawn_service::<MessageRelayService>()?;
        stack.spawn_service::<SubscribeService>()?;
        Ok(Self { stack })
    }
}

impl BridgeTask {
    #[allow(dead_code)]
    pub fn stack(&self) -> &TaskStack<BridgeBus> {
        &self.stack
    }
}