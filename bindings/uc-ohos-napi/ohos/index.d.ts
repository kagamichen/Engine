export interface OhFileMetadata {
  displayName: string
  sizeBytes: string
  mimeType?: string
}

export interface OhClipboardRepresentation {
  kind: string
  format: string
  mimeType?: string
  bytes?: Uint8Array
  handle?: string
  displayName?: string
  sizeBytes?: string
}

export interface OhClipboardSnapshot {
  observedAtMs: number
  representations: OhClipboardRepresentation[]
}

export interface OhHost {
  privateDataDirectory: string
  cacheDirectory: string
  temporaryDirectory: string
  secureStorageGet(key: string): OhHostResult<Uint8Array | null>
  secureStorageSet(key: string, value: Uint8Array): OhHostResult<void>
  secureStorageDelete(key: string): OhHostResult<void>
  fileMetadata(handle: string): OhHostResult<OhFileMetadata>
  fileReadChunk(handle: string, offset: string, maxBytes: number): OhHostResult<Uint8Array>
  fileWriteChunk(handle: string, offset: string, bytes: Uint8Array): OhHostResult<void>
  fileFinishWrite(handle: string): OhHostResult<void>
  clipboardRead(): OhHostResult<OhClipboardSnapshot> | Promise<OhHostResult<OhClipboardSnapshot>>
  clipboardWrite(snapshot: OhClipboardSnapshot): OhHostResult<void> | Promise<OhHostResult<void>>
}

export interface OhHostResult<T> {
  ok: boolean
  value?: T
  errorCategory?: string
}

export type PreparedHost = object

export interface OhHostDirectories {
  privateDataDirectory: string
  cacheDirectory: string
  temporaryDirectory: string
}

export interface OhCollectorConfig {
  traceEndpoint: string
  logEndpoint: string
  authHeaderName?: string
  authHeaderValue?: string
}

export interface OhObservabilityConfig {
  serviceVersion: string
  environment: 'development' | 'test' | 'staging' | 'production'
  appChannel: string
  remoteDiagnosticsEnabled: boolean
  collector?: OhCollectorConfig
}

export interface OhObservabilitySetup {
  reused: boolean
  remote: 'disabled' | 'ready' | 'unavailable'
  localFile: 'disabled' | 'ready' | 'unavailable'
  droppedLocalRecords: number
}

export interface OhObservabilityHealth {
  remote: 'disabled' | 'ready' | 'unavailable'
  remoteSetupFailure?: 'http_client' | 'trace_exporter' | 'log_exporter'
  localFile: 'disabled' | 'ready' | 'unavailable'
  droppedLocalRecords: number
  droppedRemoteSpans: number
  droppedRemoteLogs: number
  failedRemoteSpanBatches: number
  failedRemoteLogBatches: number
}

export interface OhObservabilitySignalSummary {
  traces: 'completed' | 'failed' | 'timed_out' | 'already_shutdown'
  logs: 'completed' | 'failed' | 'timed_out' | 'already_shutdown'
}

export interface OhSendReport {
  entryId: string
  atMs: number
  totalAccepted: number
  totalDuplicate: number
  totalOffline: number
  totalErrored: number
  totalPending: number
}

export interface OhSessionRecovery {
  unlocked: boolean
  resumed: boolean
}

export interface OhNetworkRecoveryStatus {
  phase: 'idle' | 'recovering' | 'retry_scheduled' | 'failed'
  retryable: boolean
  nextRetryInMs?: number
}

export type OhRelayMode = 'built_in' | 'custom' | 'disabled'

export interface OhRelayOverviewEntry {
  source: 'built_in' | 'custom'
  regionId?: string
  url: string
  credentialConfigured: boolean
  /** The running node is configured with this relay. It does not mean the relay is connected. */
  inEffect: boolean
}

export interface OhRelayOverview {
  savedMode: OhRelayMode
  appliedMode?: OhRelayMode
  changePending: boolean
  entries: OhRelayOverviewEntry[]
}

export interface OhLocalDevice {
  deviceId: string
  displayName: string
}

export interface OhInvitationIssued {
  invitationCode: string
  fullInvitation: string
  expiresAtMs: number
  availability: string
}

export interface OhJoinedSpace {
  sponsorDeviceId: string
  sponsorIdentityFingerprint: string
  spaceId: string
  selfDeviceId: string
  selfIdentityFingerprint: string
  migratedRecords?: string
  preservedUnreadableRecords?: string
}

export interface OhJoinSpaceStatus {
  status: 'active' | 'pending' | 'processing' | 'rejected' | 'terminated' | 'needs_attention'
  joinId: string
  joinedSpace?: OhJoinedSpace
  targetSpaceId?: string
  sponsorDeviceId?: string
  sponsorIdentityFingerprint?: string
  cancelRequested?: boolean
  peerUpgradeRequired: boolean
  rejectionReason?: string
  terminationReason?: 'cancelled' | 'expired' | 'superseded'
  attentionReason?: string
  attentionRecovery?: string
  nextRetryAtMs?: number
}

export interface OhEngineEvent {
  kind: string
  state?: string
  refreshReason?: string
  operationId?: string
  terminal?: string
  lifecycleAction?: string
  errorCode?: number
  errorCategory?: string
  retryable?: boolean
  deviceTrustRevision?: number
  networkRecoveryPhase?: 'idle' | 'recovering' | 'retry_scheduled' | 'failed'
  nextRetryInMs?: number
  rePairingScope?: 'all_devices'
  transferProgress?: OhTransferProgress
  transferStatus?: OhTransferStatus
  receiveAttempt?: OhReceiveAttempt
  profileRecovery?: OhProfileRecovery
  settingsSections?: ('general' | 'sync' | 'retention_policy' | 'security' | 'pairing' | 'keyboard_shortcuts' | 'file_sync' | 'network' | 'mobile_sync' | 'quick_panel')[]
  peerPresence?: OhPeerPresence
}

export interface OhSpaceCreated {
  spaceId: string
  selfDeviceId: string
  identityFingerprint: string
}

export interface OhActiveClipboard {
  entryId: string
  activatedBy: string
}

export interface OhHistoryEntry {
  entryId: string
  contentType: string
  preview?: string
  createdAtMs: number
}

export interface OhHistoryPage {
  entries: OhHistoryEntry[]
  nextCursor?: string
}

export interface OhDevice {
  deviceId: string
  displayName: string
  isLocal: boolean
  online: boolean
}

export interface OhContentTypes {
  text: boolean
  image: boolean
  link: boolean
  file: boolean
  codeSnippet: boolean
  richText: boolean
}

export interface OhSyncPreferences {
  sendEnabled: boolean
  receiveEnabled: boolean
  sendContentTypes: OhContentTypes
  receiveContentTypes: OhContentTypes
}

export interface OhHistoryDetail {
  entryId: string
  content: string
  sizeBytes: string
  createdAtMs: number
  activeTimeMs: number
  mimeType?: string
}

export interface OhHistoryResource {
  blobId?: string
  mimeType?: string
  sizeBytes: string
  url?: string
  inlineData?: Uint8Array
}

export interface OhBinaryResource {
  bytes: Uint8Array
  mediaType?: string
}

export interface OhDeliveryCounts {
  accepted: number
  duplicate: number
  offline: number
  errored: number
  pending: number
}

export interface OhResendOutcome {
  kind: 'completed' | 'synchronization_disabled' | 'entry_not_found' | 'entry_not_resendable' | 'target_not_trusted' | 'no_eligible_targets'
  report?: OhDeliveryCounts
  entryId?: string
  deviceId?: string
  reason?: 'remote_origin' | 'payload_lost'
}

export interface OhReceiveProgress {
  entryId: string
  attemptId: string
  state: string
  totalBytes: string
  completedBytes: string
  itemsTotal: number
  itemsCompleted: number
}

export interface OhTransferProgress {
  transferId: string
  entryId?: string
  attemptId?: string
  peerId: string
  direction: 'sending' | 'receiving'
  completedBytes: string
  totalBytes?: string
}

export interface OhTransferStatus {
  transferId: string
  entryId?: string
  attemptId?: string
  status: string
  reason?: string
}

export interface OhReceiveAttempt {
  entryId: string
  attemptId: string
  state: string
}

export interface OhPeerPresence {
  deviceId: string
  state: string
  atMs: number
}

export interface OhSetupInvitation {
  invitationCode: string
  fullInvitation: string
  expiresAtMs: number
}

export interface OhSetupState {
  hasCompleted: boolean
  spaceId?: string
  currentInvitation?: OhSetupInvitation
  deviceName?: string
  rePairingRequired: boolean
}

export interface OhEncryptionState {
  initialized: boolean
  sessionReady: boolean
}

export interface OhAdmissionRecovery {
  category: 'credential_missing' | 'authentication_mismatch' | 'current_metadata_invalid' | 'legacy_fallback_invalid' | 'legacy_migration_failed' | 'record_relation_incomplete' | 'derived_summary_invalid' | 'generation_mismatch' | 'other_storage_error'
  stage: 'credential' | 'repository_metadata' | 'legacy_repository' | 'repository_record' | 'recovery_summary' | 'storage'
  action: 'restore_credential' | 'choose_backup' | 'rebuild_derived_state' | 'export_diagnostics'
}

export interface OhProfileRecovery {
  state: 'not_required' | 'awaiting_passphrase' | 'recovering' | 'recovered' | 'partially_recoverable' | 'failed' | 'admission_recovery_required'
  canSubmitPassphrase: boolean
  restartRequired: boolean
  backgroundReady: boolean
  cleanupPending: boolean
  losses: ('local_history' | 'local_control_state' | 'device_identity')[]
  admission?: OhAdmissionRecovery
}

export interface OhSyncSettings {
  syncEnabled: boolean
  autoSyncEnabled: boolean
  syncFrequency: 'realtime' | 'interval'
  contentTypes: OhContentTypes
  syncOnRestore: boolean
}

export interface OhContentTypesPatch {
  text?: boolean
  image?: boolean
  link?: boolean
  file?: boolean
  codeSnippet?: boolean
  richText?: boolean
}

export interface OhSyncSettingsPatch {
  syncEnabled?: boolean
  autoSyncEnabled?: boolean
  syncFrequency?: 'realtime' | 'interval'
  contentTypes?: OhContentTypesPatch
  syncOnRestore?: boolean
}

export interface OhSyncSettingsUpdate {
  kind: 'updated' | 'rejected'
  settings?: OhSyncSettings
  reason?: string
}

export interface OhEngine {
  queryProfileRecovery(): Promise<OhProfileRecovery>
  querySetupState(): Promise<OhSetupState>
  queryEncryptionState(): Promise<OhEncryptionState>
  unlockSpace(passphrase: string): Promise<string>
  lockEncryption(): Promise<void>
  querySyncSettings(): Promise<OhSyncSettings>
  updateSyncSettings(patch: OhSyncSettingsPatch): Promise<OhSyncSettingsUpdate>
  getHistoryEntry(entryId: string): Promise<OhHistoryDetail>
  deleteHistoryEntry(entryId: string): Promise<void>
  getHistoryEntryResource(entryId: string): Promise<OhHistoryResource>
  readBlob(blobId: string): Promise<OhBinaryResource>
  resendEntry(entryId: string, targetDevices: string[]): Promise<OhResendOutcome>
  queryEntryReceiveProgress(entryId: string): Promise<OhReceiveProgress | null>
  listEntryReceiveProgress(): Promise<OhReceiveProgress[]>
  cancelEntryReceive(entryId: string, attemptId: string): Promise<'cancelled' | 'not_receiving' | 'too_late' | 'already_terminal' | 'superseded'>
  cancelInboundTransfer(transferId: string): Promise<'cancelled' | 'not_inflight'>
  queryMemberSyncPreferences(deviceId: string): Promise<OhSyncPreferences>
  updateMemberSyncPreferences(deviceId: string, preferences: OhSyncPreferences): Promise<OhSyncPreferences>
  queryHistory(cursor: string | null, limit: number, query: string | null): Promise<OhHistoryPage>
  listDevices(): Promise<OhDevice[]>
  observeClipboardChange(dispatch: boolean): Promise<OhSendReport | null>

  createSpace(deviceName: string | null, passphrase: string): Promise<OhSpaceCreated>
  recoverSession(allowSecureStorageUnlock: boolean): Promise<OhSessionRecovery>
  notifyConnectivityOpportunity(reason: 'foreground' | 'system_wake' | 'network_changed'): Promise<void>
  recoverNetwork(): Promise<void>
  queryNetworkRecoveryStatus(): Promise<OhNetworkRecoveryStatus>
  queryRelayOverview(): Promise<OhRelayOverview>
  queryLocalDevice(): Promise<OhLocalDevice>
  queryDeviceGroupChoices(): Promise<string>
  chooseDeviceGroup(issueId: string, choiceId: string, expectedRevision: number, confirmLocalRemoval: boolean): Promise<string>
  issueInvitation(): Promise<OhInvitationIssued>
  changeEncryptionPassphrase(passphrase: string, passphraseConfirmation: string): Promise<void>
  joinSpace(
    invitationCode: string,
    deviceName: string | null,
    passphrase: string,
    preserveUnreadableHistory: boolean
  ): Promise<OhJoinSpaceStatus>
  cancelJoinSpace(joinId: string): Promise<OhJoinSpaceStatus>
  removeMember(deviceId: string): Promise<string>
  queryActiveClipboard(): Promise<OhActiveClipboard | null>
  lifecycleState(): Promise<string>
  suspend(): Promise<void>
  suspendWithDeadline(deadlineMs: number): Promise<void>
  resume(): Promise<void>
  sendText(text: string, targetDevices: string[]): Promise<OhSendReport>
  sendImage(bytes: Uint8Array, mimeType: string, targetDevices: string[]): Promise<OhSendReport>
  sendFiles(fileHandles: string[], targetDevices: string[]): Promise<OhSendReport>
  captureCurrentClipboard(): Promise<string | null>
  restoreClipboard(entryId: string, mode: 'standard' | 'plain_text' | 'file_paths'): Promise<'restored' | 'payload_unavailable' | 'not_applicable'>
  exportEntry(entryId: string, destinationHandle: string): Promise<void>
  exportEntryFiles(entryId: string, destinationHandle: string): Promise<void>
  nextEvent(timeoutMs: number): Promise<OhEngineEvent | null>
  shutdown(deadlineMs: number): Promise<void>
  shutdownUntilComplete(): Promise<void>
}

export interface OhStartupLifecycle {
  suspendWithDeadline(deadlineMs: number): Promise<void>
  resume(): Promise<void>
}


// 本地诊断计数使用十进制字符串；只有当前进程的刷新得到确认。
export enum OhHostDiagnosticSource { Application, ShareExtension, KeyboardExtension, BackgroundService }
export enum OhHostDiagnosticAction { RuntimeStart, RuntimeStop, OwnershipAcquire, SecurityPrepare }
export enum OhHostDiagnosticFailure { Unavailable, PermissionDenied, Locked, Busy, Unknown }
export enum OhHostLifecycleState { Foreground, Background }
export enum OhHostNetworkKind { Wifi, Cellular, Ethernet, Other, Unknown }
export enum OhSourceCapability { Supported, Partial, Unsupported, Unknown }
export enum OhHostDiagnosticOutcome { Completed, Failed, Interrupted }
export interface OhLocalCaptureStatus {
  mode: string
  captureId?: string
  remainingMs: number
  startedAtUtc?: string
  endReason?: string
  lastCaptureId?: string
  revision: string
}
export interface OhSourceCoverage {
  source: string
  capability: string
  collection: string
  observedCount: string
  policyFilteredCount: string
}
export interface OhFileSourceCounts {
  source: string
  acceptedCount: string
  writtenCount: string
  queueDroppedCount: string
  quotaDroppedCount: string
  writeFailedCount: string
  lastWrittenAtMs?: string
}
export interface OhLocalDiagnosticStatus {
  runId: string
  capture: OhLocalCaptureStatus
  observedRecords: string
  policyFilteredRecords: string
  schemaRejectedRecords: string
  correlationLimitedRecords: string
  engineVersion: string
  sourceCommit: string
  counterScope: string
  sources: OhSourceCoverage[]
  localFile: string
  closed: boolean
}
export interface OhLocalDiagnosticExportReport {
  flush: string
  status: OhLocalDiagnosticStatus
  requestedAtUtc: string
  completedAtUtc: string
  otherProcessesFlushed: boolean
  files: OhFileSourceCounts[]
}
export interface OhHostDiagnosticReceipt {
  status: string
  token?: string
}

declare const engine: {
  startLocalDiagnosticCapture(durationMs: number): OhLocalCaptureStatus
  stopLocalDiagnosticCapture(captureId: string): string
  queryLocalDiagnosticStatus(): OhLocalDiagnosticStatus
  prepareLocalDiagnosticExport(deadlineMs: number): Promise<OhLocalDiagnosticExportReport>
  registerHostDiagnosticSource(source: OhHostDiagnosticSource, capability: OhSourceCapability): void
  beginHostDiagnostic(source: OhHostDiagnosticSource, action: OhHostDiagnosticAction): OhHostDiagnosticReceipt
  finishHostDiagnostic(source: OhHostDiagnosticSource, token: string, outcome: OhHostDiagnosticOutcome, reason?: OhHostDiagnosticFailure): OhHostDiagnosticReceipt
  recordHostLifecycle(source: OhHostDiagnosticSource, state: OhHostLifecycleState): OhHostDiagnosticReceipt
  recordHostNetworkChange(source: OhHostDiagnosticSource, kind: OhHostNetworkKind, available: boolean): OhHostDiagnosticReceipt
  recordHostOwnershipReleased(source: OhHostDiagnosticSource): OhHostDiagnosticReceipt
  coreVersion(): string
  installProcessObservability(
    config: OhObservabilityConfig,
    directories: OhHostDirectories
  ): OhObservabilitySetup
  queryProcessObservabilityHealth(): OhObservabilityHealth
  flushProcessObservability(deadlineMs: number): Promise<OhObservabilitySignalSummary>
  shutdownProcessObservability(deadlineMs: number): Promise<OhObservabilitySignalSummary>
  prepareHost(host: OhHost): PreparedHost
  createStartupLifecycle(): OhStartupLifecycle
  startEngine(
    config: { appVersion: string; profileId: string },
    preparedHost: PreparedHost,
    lifecycle: OhStartupLifecycle
  ): Promise<OhEngine>
}

export default engine
