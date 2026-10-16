use napi_derive::napi;
use uc_engine::{
    AdmissionRecoveryAction, AdmissionRecoveryCategory, AdmissionRecoveryStage, Operation,
    OperationResult, ProfileRecoveryLoss, ProfileRecoveryState, ProfileRecoverySummary,
    SecretString, UnlockSpaceInput,
};
use zeroize::Zeroizing;

use crate::runtime::{engine_error, unexpected_result};
use crate::OhEngine;

#[napi(object)]
pub struct OhSetupInvitation {
    pub invitation_code: String,
    pub full_invitation: String,
    pub expires_at_ms: f64,
}

#[napi(object)]
pub struct OhSetupState {
    pub has_completed: bool,
    pub space_id: Option<String>,
    pub current_invitation: Option<OhSetupInvitation>,
    pub device_name: Option<String>,
    pub re_pairing_required: bool,
}

#[napi(object)]
pub struct OhEncryptionState {
    pub initialized: bool,
    pub session_ready: bool,
}

#[napi(object)]
pub struct OhAdmissionRecovery {
    pub category: String,
    pub stage: String,
    pub action: String,
}

#[napi(object)]
pub struct OhProfileRecovery {
    pub state: String,
    pub can_submit_passphrase: bool,
    pub restart_required: bool,
    pub background_ready: bool,
    pub cleanup_pending: bool,
    pub losses: Vec<String>,
    pub admission: Option<OhAdmissionRecovery>,
}

#[napi]
impl OhEngine {
    #[napi]
    pub async fn query_profile_recovery(&self) -> napi::Result<OhProfileRecovery> {
        match self
            .engine
            .execute(Operation::QueryProfileRecovery)
            .await
            .map_err(engine_error)?
        {
            OperationResult::ProfileRecovery(summary) => Ok(profile_recovery(summary)),
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn query_setup_state(&self) -> napi::Result<OhSetupState> {
        match self
            .engine
            .execute(Operation::QuerySetupState)
            .await
            .map_err(engine_error)?
        {
            OperationResult::SetupState(state) => Ok(OhSetupState {
                has_completed: state.has_completed,
                space_id: state.space_id,
                current_invitation: state
                    .current_invitation
                    .map(|invitation| OhSetupInvitation {
                        invitation_code: invitation.invitation_code,
                        full_invitation: invitation.full_invitation,
                        expires_at_ms: invitation.expires_at_ms as f64,
                    }),
                device_name: state.device_name,
                re_pairing_required: state.re_pairing_required,
            }),
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn query_encryption_state(&self) -> napi::Result<OhEncryptionState> {
        match self
            .engine
            .execute(Operation::QueryEncryptionState)
            .await
            .map_err(engine_error)?
        {
            OperationResult::EncryptionState(state) => Ok(OhEncryptionState {
                initialized: state.initialized,
                session_ready: state.session_ready,
            }),
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn unlock_space(&self, passphrase: String) -> napi::Result<String> {
        let passphrase = Zeroizing::new(passphrase);
        match self
            .engine
            .execute(Operation::UnlockSpace(UnlockSpaceInput {
                passphrase: SecretString::new(passphrase.as_str()),
            }))
            .await
            .map_err(engine_error)?
        {
            OperationResult::SpaceUnlocked { space_id } => Ok(space_id),
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn lock_encryption(&self) -> napi::Result<()> {
        match self
            .engine
            .execute(Operation::LockEncryption)
            .await
            .map_err(engine_error)?
        {
            OperationResult::EncryptionLocked => Ok(()),
            _ => Err(unexpected_result()),
        }
    }
}

pub(crate) fn profile_recovery(summary: ProfileRecoverySummary) -> OhProfileRecovery {
    OhProfileRecovery {
        state: match summary.state {
            ProfileRecoveryState::NotRequired => "not_required",
            ProfileRecoveryState::AwaitingPassphrase => "awaiting_passphrase",
            ProfileRecoveryState::Recovering => "recovering",
            ProfileRecoveryState::Recovered => "recovered",
            ProfileRecoveryState::PartiallyRecoverable => "partially_recoverable",
            ProfileRecoveryState::Failed => "failed",
            ProfileRecoveryState::AdmissionRecoveryRequired => "admission_recovery_required",
        }
        .to_owned(),
        can_submit_passphrase: summary.can_submit_passphrase,
        restart_required: summary.restart_required,
        background_ready: summary.background_ready,
        cleanup_pending: summary.cleanup_pending,
        losses: summary
            .losses
            .into_iter()
            .map(|loss| {
                match loss {
                    ProfileRecoveryLoss::LocalHistory => "local_history",
                    ProfileRecoveryLoss::LocalControlState => "local_control_state",
                    ProfileRecoveryLoss::DeviceIdentity => "device_identity",
                }
                .to_owned()
            })
            .collect(),
        admission: summary.admission.map(|admission| OhAdmissionRecovery {
            category: match admission.category {
                AdmissionRecoveryCategory::CredentialMissing => "credential_missing",
                AdmissionRecoveryCategory::AuthenticationMismatch => "authentication_mismatch",
                AdmissionRecoveryCategory::CurrentMetadataInvalid => "current_metadata_invalid",
                AdmissionRecoveryCategory::LegacyFallbackInvalid => "legacy_fallback_invalid",
                AdmissionRecoveryCategory::LegacyMigrationFailed => "legacy_migration_failed",
                AdmissionRecoveryCategory::RecordRelationIncomplete => "record_relation_incomplete",
                AdmissionRecoveryCategory::DerivedSummaryInvalid => "derived_summary_invalid",
                AdmissionRecoveryCategory::GenerationMismatch => "generation_mismatch",
                AdmissionRecoveryCategory::OtherStorageError => "other_storage_error",
            }
            .to_owned(),
            stage: match admission.stage {
                AdmissionRecoveryStage::Credential => "credential",
                AdmissionRecoveryStage::RepositoryMetadata => "repository_metadata",
                AdmissionRecoveryStage::LegacyRepository => "legacy_repository",
                AdmissionRecoveryStage::RepositoryRecord => "repository_record",
                AdmissionRecoveryStage::RecoverySummary => "recovery_summary",
                AdmissionRecoveryStage::Storage => "storage",
            }
            .to_owned(),
            action: match admission.action {
                AdmissionRecoveryAction::RestoreCredential => "restore_credential",
                AdmissionRecoveryAction::ChooseBackup => "choose_backup",
                AdmissionRecoveryAction::RebuildDerivedState => "rebuild_derived_state",
                AdmissionRecoveryAction::ExportDiagnostics => "export_diagnostics",
            }
            .to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uc_engine::AdmissionRecoverySummary;

    #[test]
    fn partial_recovery_keeps_loss_and_restart_requirements() {
        let result = profile_recovery(ProfileRecoverySummary {
            state: ProfileRecoveryState::PartiallyRecoverable,
            can_submit_passphrase: false,
            restart_required: true,
            background_ready: false,
            cleanup_pending: true,
            losses: vec![
                ProfileRecoveryLoss::LocalHistory,
                ProfileRecoveryLoss::DeviceIdentity,
            ],
            admission: None,
        });
        assert_eq!(result.state, "partially_recoverable");
        assert!(!result.can_submit_passphrase);
        assert!(!result.background_ready);
        assert!(result.restart_required);
        assert!(result.cleanup_pending);
        assert_eq!(result.losses, vec!["local_history", "device_identity"]);
    }

    #[test]
    fn admission_recovery_is_not_reported_as_a_passphrase_retry() {
        let result = profile_recovery(ProfileRecoverySummary {
            state: ProfileRecoveryState::AdmissionRecoveryRequired,
            can_submit_passphrase: false,
            restart_required: false,
            background_ready: false,
            cleanup_pending: false,
            losses: Vec::new(),
            admission: Some(AdmissionRecoverySummary {
                category: AdmissionRecoveryCategory::AuthenticationMismatch,
                stage: AdmissionRecoveryStage::RepositoryRecord,
                action: AdmissionRecoveryAction::ChooseBackup,
            }),
        });
        assert_eq!(result.state, "admission_recovery_required");
        assert!(!result.can_submit_passphrase);
        let admission = result.admission.unwrap();
        assert_eq!(admission.category, "authentication_mismatch");
        assert_eq!(admission.stage, "repository_record");
        assert_eq!(admission.action, "choose_backup");
    }
}
