use napi::Status;
use napi_derive::napi;
use uc_engine::{
    Operation, OperationResult, SettingsContentTypesPatch, SettingsPatch, SettingsSectionSummary,
    SettingsUpdateOutcome, SyncFrequencySummary, SyncSettingsPatch, SyncSettingsSummary,
};

use crate::runtime::{engine_error, unexpected_result};
use crate::{OhContentTypes, OhEngine};

#[napi(object)]
pub struct OhSyncSettings {
    pub sync_enabled: bool,
    pub auto_sync_enabled: bool,
    pub sync_frequency: String,
    pub content_types: OhContentTypes,
    pub sync_on_restore: bool,
}

#[napi(object)]
pub struct OhContentTypesPatch {
    pub text: Option<bool>,
    pub image: Option<bool>,
    pub link: Option<bool>,
    pub file: Option<bool>,
    pub code_snippet: Option<bool>,
    pub rich_text: Option<bool>,
}

#[napi(object)]
pub struct OhSyncSettingsPatch {
    pub sync_enabled: Option<bool>,
    pub auto_sync_enabled: Option<bool>,
    pub sync_frequency: Option<String>,
    pub content_types: Option<OhContentTypesPatch>,
    pub sync_on_restore: Option<bool>,
}

#[napi(object)]
pub struct OhSyncSettingsUpdate {
    pub kind: String,
    pub settings: Option<OhSyncSettings>,
    pub reason: Option<String>,
}

#[napi]
impl OhEngine {
    #[napi]
    pub async fn query_sync_settings(&self) -> napi::Result<OhSyncSettings> {
        match self
            .engine
            .execute(Operation::QuerySettings)
            .await
            .map_err(engine_error)?
        {
            OperationResult::Settings(settings) => Ok(sync_settings(settings.sync)),
            _ => Err(unexpected_result()),
        }
    }

    #[napi]
    pub async fn update_sync_settings(
        &self,
        patch: OhSyncSettingsPatch,
    ) -> napi::Result<OhSyncSettingsUpdate> {
        let patch = SettingsPatch {
            sync: Some(SyncSettingsPatch {
                sync_enabled: patch.sync_enabled,
                auto_sync_enabled: patch.auto_sync_enabled,
                sync_frequency: patch
                    .sync_frequency
                    .as_deref()
                    .map(parse_frequency)
                    .transpose()?,
                content_types: patch.content_types.map(|types| SettingsContentTypesPatch {
                    text: types.text,
                    image: types.image,
                    link: types.link,
                    file: types.file,
                    code_snippet: types.code_snippet,
                    rich_text: types.rich_text,
                }),
                sync_on_restore: patch.sync_on_restore,
            }),
            ..SettingsPatch::default()
        };
        match self
            .engine
            .execute(Operation::UpdateSettings(Box::new(patch)))
            .await
            .map_err(engine_error)?
        {
            OperationResult::SettingsUpdated(outcome) => Ok(update_outcome(outcome)),
            _ => Err(unexpected_result()),
        }
    }
}

fn parse_frequency(value: &str) -> napi::Result<SyncFrequencySummary> {
    match value {
        "realtime" => Ok(SyncFrequencySummary::Realtime),
        "interval" => Ok(SyncFrequencySummary::Interval),
        _ => Err(napi::Error::new(
            Status::InvalidArg,
            "OHOS_INVALID_SYNC_FREQUENCY",
        )),
    }
}

fn sync_settings(settings: SyncSettingsSummary) -> OhSyncSettings {
    OhSyncSettings {
        sync_enabled: settings.sync_enabled,
        auto_sync_enabled: settings.auto_sync_enabled,
        sync_frequency: match settings.sync_frequency {
            SyncFrequencySummary::Realtime => "realtime",
            SyncFrequencySummary::Interval => "interval",
        }
        .to_owned(),
        content_types: OhContentTypes {
            text: settings.content_types.text,
            image: settings.content_types.image,
            link: settings.content_types.link,
            file: settings.content_types.file,
            code_snippet: settings.content_types.code_snippet,
            rich_text: settings.content_types.rich_text,
        },
        sync_on_restore: settings.sync_on_restore,
    }
}

fn update_outcome(outcome: SettingsUpdateOutcome) -> OhSyncSettingsUpdate {
    match outcome {
        SettingsUpdateOutcome::Updated(settings) => OhSyncSettingsUpdate {
            kind: "updated".to_owned(),
            settings: Some(sync_settings(settings.sync)),
            reason: None,
        },
        SettingsUpdateOutcome::Rejected { reason } => OhSyncSettingsUpdate {
            kind: "rejected".to_owned(),
            settings: None,
            reason: Some(reason),
        },
    }
}

pub(crate) fn settings_section(section: SettingsSectionSummary) -> &'static str {
    match section {
        SettingsSectionSummary::General => "general",
        SettingsSectionSummary::Sync => "sync",
        SettingsSectionSummary::RetentionPolicy => "retention_policy",
        SettingsSectionSummary::Security => "security",
        SettingsSectionSummary::Pairing => "pairing",
        SettingsSectionSummary::KeyboardShortcuts => "keyboard_shortcuts",
        SettingsSectionSummary::FileSync => "file_sync",
        SettingsSectionSummary::Network => "network",
        SettingsSectionSummary::MobileSync => "mobile_sync",
        SettingsSectionSummary::QuickPanel => "quick_panel",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_update_does_not_fabricate_saved_settings() {
        let result = update_outcome(SettingsUpdateOutcome::Rejected {
            reason: "synthetic_refusal".to_owned(),
        });
        assert_eq!(result.kind, "rejected");
        assert!(result.settings.is_none());
        assert_eq!(result.reason.as_deref(), Some("synthetic_refusal"));
    }

    #[test]
    fn unknown_frequency_cannot_silently_become_realtime() {
        assert!(matches!(
            parse_frequency("realtime").unwrap(),
            SyncFrequencySummary::Realtime
        ));
        assert!(matches!(
            parse_frequency("interval").unwrap(),
            SyncFrequencySummary::Interval
        ));
        assert_eq!(
            parse_frequency("unknown").unwrap_err().status,
            Status::InvalidArg
        );
    }
}
