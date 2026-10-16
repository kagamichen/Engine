//! 负责本机捕获后的自动、显式投递与对端恢复。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

mod recovery;
mod shutdown;

use recovery::{OfflineDeliveryRecovery, OfflineDeliveryRecoveryDeps, RecoveryDeliveryPort};

use uc_core::ids::{DeviceId, EntryId};
use uc_core::ports::clipboard::{ClipboardEventRepositoryPort, ListClipboardEntriesPort};
use uc_core::ports::{
    ClockPort, DeviceIdentityPort, EntryDeliveryRepositoryPort, PeerAddressRepositoryPort,
    PeerReachabilityPort, SettingsPort,
};
use uc_observability_contract::diagnostics::connectivity::{
    LocalWorkObservation, LocalWorkOutcome, LocalWorkStep,
};

use crate::clipboard::inbound::ClipboardInboundRuntime;
use crate::clipboard::outbound::{
    ClipboardOutboundError, ClipboardOutboundFacade, ClipboardOutboundInput,
    ClipboardOutboundOutcome, ClipboardOutboundPort,
};
use crate::deps::CurrentSpaceMemberScopePort;
use crate::runtime_lifecycle::LifecycleError;
use uc_observability_contract::uc_warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipboardDeliveryIntent {
    Automatic,
    Explicit,
}

/// 本机捕获出站的完整生命周期；历史重发使用独立 facade，
/// 离线恢复保持为本运行期的内部责任。
pub struct ClipboardSyncRuntime {
    outbound: Arc<dyn ClipboardOutboundPort>,
    settings: Arc<dyn SettingsPort>,
    inbound: tokio::sync::Mutex<Option<ClipboardInboundRuntime>>,
    delivery_gate: Arc<tokio::sync::Mutex<()>>,
    recovery: OfflineDeliveryRecovery,
    stopping: AtomicBool,
    shutdown_result: tokio::sync::Mutex<Option<Result<(), Arc<LifecycleError>>>>,
}

pub struct ClipboardSyncRuntimeDeps {
    pub outbound: Arc<ClipboardOutboundFacade>,
    pub settings: Arc<dyn SettingsPort>,
    pub inbound: ClipboardInboundRuntime,
    pub peer_reachability: Arc<dyn PeerReachabilityPort>,
    pub known_peers: Arc<dyn PeerAddressRepositoryPort>,
    pub member_scope: Arc<dyn CurrentSpaceMemberScopePort>,
    pub entries: Arc<dyn ListClipboardEntriesPort>,
    pub events: Arc<dyn ClipboardEventRepositoryPort>,
    pub deliveries: Arc<dyn EntryDeliveryRepositoryPort>,
    pub device_identity: Arc<dyn DeviceIdentityPort>,
    pub clock: Arc<dyn ClockPort>,
}

impl ClipboardSyncRuntime {
    pub fn start(deps: ClipboardSyncRuntimeDeps) -> Self {
        let delivery_gate = Arc::new(tokio::sync::Mutex::new(()));
        let recovery = OfflineDeliveryRecovery::start(OfflineDeliveryRecoveryDeps {
            peer_reachability: deps.peer_reachability,
            known_peers: deps.known_peers,
            member_scope: deps.member_scope,
            settings: Arc::clone(&deps.settings),
            entries: deps.entries,
            events: deps.events,
            deliveries: deps.deliveries,
            device_identity: deps.device_identity,
            clock: deps.clock,
            delivery: Arc::clone(&deps.outbound) as Arc<dyn RecoveryDeliveryPort>,
            delivery_gate: Arc::clone(&delivery_gate),
        });
        Self {
            outbound: deps.outbound,
            settings: deps.settings,
            inbound: tokio::sync::Mutex::new(Some(deps.inbound)),
            delivery_gate,
            recovery,
            stopping: AtomicBool::new(false),
            shutdown_result: tokio::sync::Mutex::new(None),
        }
    }

    /// 按调用意图投递本机捕获，并保留显式目标筛选。
    pub(crate) async fn dispatch_local_capture(
        &self,
        input: ClipboardOutboundInput,
        target_filter: Option<Vec<DeviceId>>,
        intent: ClipboardDeliveryIntent,
    ) -> Result<ClipboardOutboundOutcome, ClipboardOutboundError> {
        let gate_observation =
            LocalWorkObservation::begin(LocalWorkStep::ClipboardDeliveryGateWait);
        let _gate = self.delivery_gate.lock().await;
        gate_observation.finish(LocalWorkOutcome::Ok);
        if self.stopping.load(Ordering::Acquire) {
            return Ok(ClipboardOutboundOutcome::Skipped {
                reason: "runtime_stopped".to_owned(),
            });
        }
        if !delivery_enabled(self.settings.as_ref(), intent).await {
            return Ok(ClipboardOutboundOutcome::Skipped {
                reason: "sync_disabled".to_string(),
            });
        }
        let entry_id = EntryId::from(input.entry_id.as_str());
        let outcome = self.outbound.dispatch_capture(input, target_filter).await?;
        if let ClipboardOutboundOutcome::Dispatched {
            per_target,
            pending_targets,
            ..
        } = &outcome
        {
            let mut targets: Vec<DeviceId> =
                per_target.iter().map(|target| target.device_id).collect();
            targets.extend(pending_targets.iter().copied());
            self.recovery
                .supersede_older_recoverable_entries(&entry_id, &targets)
                .await;
        }
        Ok(outcome)
    }
}

async fn delivery_enabled(settings: &dyn SettingsPort, intent: ClipboardDeliveryIntent) -> bool {
    let observation = LocalWorkObservation::begin(LocalWorkStep::ClipboardSyncSettingsLoad);
    let result = settings.load().await;
    observation.finish(if result.is_ok() {
        LocalWorkOutcome::Ok
    } else {
        LocalWorkOutcome::Error
    });
    match result {
        Ok(settings) => {
            settings.sync.sync_enabled
                && (intent == ClipboardDeliveryIntent::Explicit || settings.sync.auto_sync_enabled)
        }
        Err(_) => {
            uc_warn!(
                error_kind = "settings_load",
                "clipboard sync: delivery skipped"
            );
            false
        }
    }
}
