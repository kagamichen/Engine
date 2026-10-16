const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

async function main() {
  const addonPath = process.env.UC_OHOS_NAPI_NODE;
  assert.ok(addonPath, 'UC_OHOS_NAPI_NODE must point to the built N-API module');

  const addon = require(addonPath);
  require('./declaration_contract.cjs').assertDeclaredMethods(addon);
  assert.equal(addon.coreVersion(), 'v1.1.0-rc.22');
  assert.equal(typeof addon.installProcessObservability, 'function');
  assert.equal(typeof addon.queryProcessObservabilityHealth, 'function');
  assert.equal(typeof addon.flushProcessObservability, 'function');
  assert.equal(typeof addon.shutdownProcessObservability, 'function');
  assert.equal(typeof addon.prepareHost, 'function');
  assert.equal(typeof addon.startEngine, 'function');

  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'uc-ohos-napi-'));
  const values = new Map();
  const files = new Map([
    [
      'input-file-1',
      {
        displayName: 'private-name.txt',
        mimeType: 'text/plain',
        bytes: Buffer.from('private HarmonyOS binding file'),
        finished: false,
      },
    ],
    [
      'output-file-1',
      {
        displayName: 'private-export.txt',
        mimeType: 'text/plain',
        bytes: Buffer.alloc(0),
        finished: false,
      },
    ],
  ]);
  const privateClipboardText = Buffer.from('private HarmonyOS clipboard text');
  files.set('empty-input', {
    displayName: 'empty.txt', mimeType: 'text/plain', bytes: Buffer.alloc(0), finished: false
  });
  files.set('empty-output', {
    displayName: 'empty-export.txt', mimeType: 'text/plain', bytes: Buffer.alloc(0), finished: false
  });
  let clipboard = {
    observedAtMs: 1_700_000_000_000,
    representations: [
      {
        kind: 'inline',
        format: 'text/plain',
        mimeType: 'text/plain',
        bytes: privateClipboardText,
      },
    ],
  };
  const clipboardWrites = [];
  const binaryShapes = [];
  const recordBinaryShape = (source, value) => {
    binaryShapes.push({ source, typedArray: value instanceof Uint8Array, nodeBuffer: Buffer.isBuffer(value) });
  };
  let readOverride;
  let writeOverride;
  const host = {
    privateDataDirectory: path.join(root, 'data'),
    cacheDirectory: path.join(root, 'cache'),
    temporaryDirectory: path.join(root, 'temporary'),
    secureStorageGet(key) {
      const value = values.get(key);
      return success(value === undefined ? null : new Uint8Array(value));
    },
    secureStorageSet(key, value) {
      recordBinaryShape('secureStorageSet', value);
      values.set(key, Buffer.from(value));
      return success();
    },
    secureStorageDelete(key) {
      values.delete(key);
      return success();
    },
    fileMetadata(handle) {
      const file = requireFile(files, handle);
      return success({
        displayName: file.displayName,
        sizeBytes: String(file.bytes.length),
        mimeType: file.mimeType,
      });
    },
    fileReadChunk(handle, offset, maxBytes) {
      const file = requireFile(files, handle);
      const start = Number(offset);
      return success(new Uint8Array(file.bytes.subarray(start, start + maxBytes)));
    },
    fileWriteChunk(handle, offset, bytes) {
      recordBinaryShape('fileWriteChunk', bytes);
      const file = requireFile(files, handle);
      const start = Number(offset);
      const end = start + bytes.length;
      if (file.bytes.length < end) {
        const expanded = Buffer.alloc(end);
        file.bytes.copy(expanded);
        file.bytes = expanded;
      }
      Buffer.from(bytes).copy(file.bytes, start);
      return success();
    },
    fileFinishWrite(handle) {
      requireFile(files, handle).finished = true;
      return success();
    },
    clipboardRead() {
      const result = success({
        ...clipboard,
        representations: clipboard.representations.map((representation) => ({
          ...representation,
          bytes: representation.bytes === undefined
            ? undefined
            : new Uint8Array(representation.bytes),
        })),
      });
      return readOverride ? readOverride(result) : result;
    },
    clipboardWrite(snapshot) {
      for (const representation of snapshot.representations) {
        if (representation.kind === 'inline') recordBinaryShape('clipboardWrite', representation.bytes);
      }
      const complete = () => {
        clipboardWrites.push(snapshot);
        clipboard = snapshot;
        return success();
      };
      return writeOverride ? writeOverride(complete) : complete();
    },
  };

  try {
    const observabilityConfig = {
      serviceVersion: '1.2.3',
      environment: 'test',
      appChannel: 'test',
      remoteDiagnosticsEnabled: false,
    };
    const hostDirectories = {
      privateDataDirectory: host.privateDataDirectory,
      cacheDirectory: host.cacheDirectory,
      temporaryDirectory: host.temporaryDirectory,
    };
    const setup = addon.installProcessObservability(observabilityConfig, hostDirectories);
    assert.equal(setup.reused, false);
    assert.equal(setup.remote, 'disabled');
    assert.equal(setup.localFile, 'ready');
    const health = addon.queryProcessObservabilityHealth();
    assert.equal(health.remote, setup.remote);
    assert.equal(health.localFile, setup.localFile);
    for (const counter of [
      health.droppedLocalRecords,
      health.droppedRemoteSpans,
      health.droppedRemoteLogs,
      health.failedRemoteSpanBatches,
      health.failedRemoteLogBatches,
    ]) {
      assert.equal(Number.isSafeInteger(counter), true);
      assert.ok(counter >= 0);
    }

    const preparedHost = addon.prepareHost(host);
    const engine = await addon.startEngine(
      { appVersion: '1.2.3', profileId: 'ohos-host-smoke' },
      preparedHost,
      addon.createStartupLifecycle()
    );
    assert.equal((await engine.querySetupState()).hasCompleted, false);
    assert.deepEqual(await engine.queryEncryptionState(), { initialized: false, sessionReady: false });
    const created = await engine.createSpace(
      'ohos-host-smoke',
      'correct horse battery staple'
    );
    assert.ok(created.spaceId);
    assert.ok(created.selfDeviceId);
    assert.ok(created.identityFingerprint);
    assert.ok(values.size > 0, 'space secrets must be persisted through the host callback');
    const configured = await engine.querySetupState();
    assert.equal(configured.hasCompleted, true);
    assert.equal(configured.spaceId, created.spaceId);
    assert.equal(configured.rePairingRequired, false);
    assert.deepEqual(await engine.queryEncryptionState(), { initialized: true, sessionReady: true });

    const originalSync = await engine.querySyncSettings();
    const updatedSync = await engine.updateSyncSettings({
      autoSyncEnabled: !originalSync.autoSyncEnabled,
      contentTypes: { image: !originalSync.contentTypes.image }
    });
    assert.equal(updatedSync.kind, 'updated');
    assert.deepEqual(updatedSync.settings, {
      ...originalSync, autoSyncEnabled: !originalSync.autoSyncEnabled,
      contentTypes: { ...originalSync.contentTypes, image: !originalSync.contentTypes.image }
    });
    assert.deepEqual(await engine.querySyncSettings(), updatedSync.settings);
    await assert.rejects(engine.updateSyncSettings({ syncFrequency: 'unknown' }), /OHOS_INVALID_SYNC_FREQUENCY/);
    assert.deepEqual(await engine.querySyncSettings(), updatedSync.settings, 'invalid frequency must not mutate settings');
    let settingsEvent;
    const settingsDeadline = Date.now() + 5_000;
    while (Date.now() < settingsDeadline) {
      const event = await engine.nextEvent(100);
      if (event?.kind === 'settings_changed' && event.settingsSections?.includes('sync')) {
        settingsEvent = event;
        break;
      }
    }
    assert.deepEqual(settingsEvent?.settingsSections, ['sync']);
    assert.equal((await engine.updateSyncSettings(originalSync)).kind, 'updated');

    const invitation = await engine.issueInvitation();
    assert.ok(invitation.invitationCode);
    assert.ok(invitation.expiresAtMs > 0);
    assert.match(invitation.availability, /^(cross_network|same_local_network)$/);
    assert.equal((await engine.querySetupState()).currentInvitation.invitationCode, invitation.invitationCode);

    await assert.rejects(
      engine.joinSpace(invitation.invitationCode, '  ', 'correct horse battery staple', false),
      /UC_ENGINE:\d+:invalid_input:false/
    );

    assert.equal((await engine.updateSyncSettings({ autoSyncEnabled: false })).kind, 'updated');
    const report = await engine.sendText('private HarmonyOS binding text', []);
    assert.ok(report.entryId);
    assert.ok(report.atMs > 0);
    assert.equal(report.totalAccepted, 0);
    assert.equal(report.totalDuplicate, 0);
    assert.equal(report.totalOffline, 0);
    assert.equal(report.totalErrored, 0);
    assert.equal(report.totalPending, 0);

    const privateImage = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
    const imageView = new Uint8Array([0, ...privateImage, 0]).subarray(1, privateImage.length + 1);
    assert.equal(Buffer.isBuffer(imageView), false);
    const imageReport = await engine.sendImage(imageView, 'image/png', []);
    assert.ok(imageReport.entryId);
    assert.ok(imageReport.atMs > 0);
    assert.equal(imageReport.totalAccepted, 0);
    assert.equal(imageReport.totalPending, 0);

    const detail = await engine.getHistoryEntry(report.entryId);
    assert.equal(detail.content, 'private HarmonyOS binding text');
    assert.equal(detail.sizeBytes, String(Buffer.byteLength(detail.content)));
    const imageResource = await engine.getHistoryEntryResource(imageReport.entryId);
    const imageBytes = imageResource.inlineData ?? (await engine.readBlob(imageResource.blobId)).bytes;
    assert.ok(imageBytes instanceof Uint8Array);
    assert.equal(Buffer.isBuffer(imageBytes), false, 'public binary resources must use Uint8Array, not Node Buffer');
    recordBinaryShape('historyResource', imageBytes);
    assert.deepEqual(Buffer.from(imageBytes), privateImage);
    assert.equal(imageResource.mimeType, 'image/png');
    assert.equal((await engine.resendEntry(report.entryId, [])).kind, 'no_eligible_targets');
    assert.equal(await engine.queryEntryReceiveProgress(report.entryId), null);
    assert.deepEqual(await engine.listEntryReceiveProgress(), []);
    assert.equal(await engine.cancelEntryReceive(report.entryId, 'unused-attempt'), 'not_receiving');
    assert.equal(await engine.cancelInboundTransfer('unused-transfer'), 'not_inflight');
    await assert.rejects(engine.cancelEntryReceive('', ''), /UC_ENGINE:/);

    const fileReport = await engine.sendFiles(['input-file-1'], []);
    assert.ok(fileReport.entryId);
    assert.ok(fileReport.atMs > 0);
    assert.equal(fileReport.totalAccepted, 0);
    assert.equal(fileReport.totalPending, 0);

    const emptyReport = await engine.sendFiles(['empty-input'], []);
    assert.ok(emptyReport.entryId);
    await engine.exportEntry(emptyReport.entryId, 'empty-output');
    assert.equal(requireFile(files, 'empty-output').bytes.length, 0);
    assert.equal(requireFile(files, 'empty-output').finished, true);

    files.set('second-input', {
      displayName: 'private-name.txt', mimeType: 'text/plain', bytes: Buffer.from('second file'), finished: false
    });
    files.set('archive-output', {
      displayName: 'files.zip', mimeType: 'application/zip', bytes: Buffer.alloc(0), finished: false
    });
    const multipleReport = await engine.sendFiles(['input-file-1', 'second-input', 'empty-input'], []);
    await engine.exportEntryFiles(multipleReport.entryId, 'archive-output');
    const archive = requireFile(files, 'archive-output');
    assert.equal(archive.finished, true);
    const parsedArchive = require('node:child_process').execFileSync('python3', ['-c',
      'import sys,io,zipfile,json,base64; z=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read())); assert z.testzip() is None; print(json.dumps({n:base64.b64encode(z.read(n)).decode() for n in z.namelist()}))'
    ], { input: archive.bytes, encoding: 'utf8' });
    assert.deepEqual(JSON.parse(parsedArchive), {
      'private-name.txt': requireFile(files, 'input-file-1').bytes.toString('base64'),
      'private-name (2).txt': Buffer.from('second file').toString('base64'),
      'empty.txt': ''
    });

    assert.equal((await engine.updateSyncSettings(originalSync)).kind, 'updated');

    const clipboardEntryId = await engine.captureCurrentClipboard();
    assert.ok(clipboardEntryId);
    const restoreOutcome = await engine.restoreClipboard(clipboardEntryId, 'standard');
    assert.equal(restoreOutcome, 'restored');
    assert.equal(clipboardWrites.length, 1);
    assert.equal(clipboardWrites[0].representations.length, 1);
    assert.deepEqual(
      Buffer.from(clipboardWrites[0].representations[0].bytes),
      privateClipboardText
    );
    const plainTextOutcome = await engine.restoreClipboard(clipboardEntryId, 'plain_text');
    assert.equal(plainTextOutcome, 'restored');
    assert.equal(clipboardWrites.length, 2);
    for (const shape of binaryShapes) {
      assert.equal(shape.typedArray, true, shape.source);
      assert.equal(shape.nodeBuffer, false, `${shape.source} must produce Uint8Array`);
    }
    for (const source of ['secureStorageSet', 'fileWriteChunk', 'clipboardWrite', 'historyResource']) {
      assert.ok(binaryShapes.some(shape => shape.source === source), `${source} was exercised`);
    }
    const filePathsOutcome = await engine.restoreClipboard(clipboardEntryId, 'file_paths');
    assert.equal(filePathsOutcome, 'not_applicable');
    assert.equal(clipboardWrites.length, 2);

    await engine.exportEntry(clipboardEntryId, 'output-file-1');
    assert.deepEqual(requireFile(files, 'output-file-1').bytes, privateClipboardText);
    assert.equal(requireFile(files, 'output-file-1').finished, true);

    const devices = await engine.listDevices();
    assert.equal(devices.length, 1);
    assert.equal(devices[0].isLocal, true);
    const page = await engine.queryHistory(null, 1, null);
    assert.equal(page.entries.length, 1);
    assert.ok(page.nextCursor);
    const nextPage = await engine.queryHistory(page.nextCursor, 1, null);
    assert.equal(nextPage.entries.length, 1);
    assert.notEqual(page.entries[0].entryId, nextPage.entries[0].entryId);
    const results = await engine.queryHistory(null, 50, 'binding');
    assert.ok(results.entries.some(entry => entry.entryId === report.entryId));
    await assert.rejects(engine.queryHistory(null, 0, null), /UC_ENGINE:/);
    await assert.rejects(engine.queryHistory('invalid cursor', 10, null), /UC_ENGINE:/);
    const before = await engine.queryHistory(null, 50, null);
    await engine.observeClipboardChange(false);
    const after = await engine.queryHistory(null, 50, null);
    assert.equal(after.entries.length, before.entries.length, 'remote restore must not duplicate history');

    clipboard = { observedAtMs: Date.now(), representations: [{
      kind: 'inline', format: 'text/plain', mimeType: 'text/plain',
      bytes: Buffer.from('synthetic observed clipboard content')
    }] };
    await engine.observeClipboardChange(false);
    const observed = await engine.queryHistory(null, 50, 'observed');
    assert.equal(observed.entries.length, 1, 'unified observation must index captured text');
    await engine.observeClipboardChange(false);
    const repeated = await engine.queryHistory(null, 50, 'observed');
    assert.equal(repeated.entries.length, 1, 'repeated observation must deduplicate');
    assert.equal(repeated.entries[0].entryId, observed.entries[0].entryId);
    await engine.restoreClipboard(observed.entries[0].entryId, 'standard');
    assert.equal(Buffer.from(clipboard.representations[0].bytes).toString(), 'synthetic observed clipboard content');
    await engine.deleteHistoryEntry(observed.entries[0].entryId);
    await assert.rejects(engine.getHistoryEntry(observed.entries[0].entryId), /UC_ENGINE:/);
    assert.equal((await engine.queryHistory(null, 50, 'observed')).entries.length, 0);


    readOverride = (result) => new Promise((resolve) => setTimeout(() => resolve(result), 10));
    writeOverride = (complete) => new Promise((resolve) => setTimeout(() => resolve(complete()), 10));
    const asyncEntry = await engine.captureCurrentClipboard();
    assert.ok(asyncEntry);
    const writesBeforeAsync = clipboardWrites.length;
    assert.equal(await engine.restoreClipboard(asyncEntry, 'standard'), 'restored');
    assert.equal(clipboardWrites.length, writesBeforeAsync + 1);
    assert.equal(Buffer.from(clipboard.representations[0].bytes).toString(), 'synthetic observed clipboard content');

    const textClipboard = clipboard;
    const asyncPng = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6ZpUAAAAASUVORK5CYII=', 'base64');
    clipboard = { observedAtMs: Date.now(), representations: [{
      kind: 'inline', format: 'image/png', mimeType: 'image/png', bytes: new Uint8Array(asyncPng),
    }] };
    const asyncImageEntry = await engine.captureCurrentClipboard();
    assert.ok(asyncImageEntry);
    assert.equal(await engine.restoreClipboard(asyncImageEntry, 'standard'), 'restored');
    assert.deepEqual(Buffer.from(clipboard.representations[0].bytes), asyncPng);
    clipboard = textClipboard;
    const writesBeforeRejection = clipboardWrites.length;

    const rejectedHostMarker = 'private async host rejection marker';
    readOverride = () => Promise.reject(new Error(rejectedHostMarker));
    await assert.rejects(engine.captureCurrentClipboard(), /UC_ENGINE:/);
    readOverride = () => Promise.resolve({ ok: false, errorCategory: 'permission_denied' });
    await assert.rejects(engine.captureCurrentClipboard(), /UC_ENGINE:/);
    readOverride = () => Promise.resolve(null);
    await assert.rejects(engine.captureCurrentClipboard(), /UC_ENGINE:/);
    readOverride = () => Promise.resolve({ ok: true, get value() { throw new Error(rejectedHostMarker); } });
    await assert.rejects(engine.captureCurrentClipboard(), /UC_ENGINE:/);
    readOverride = undefined;
    writeOverride = () => Promise.reject(new Error(rejectedHostMarker));
    await assert.rejects(engine.restoreClipboard(asyncEntry, 'standard'), /UC_ENGINE:/);
    assert.equal(clipboardWrites.length, writesBeforeRejection, 'rejected write must not apply clipboard');
    writeOverride = undefined;

    let lateValueReads = 0;
    readOverride = (result) => new Promise((resolve) => setTimeout(() => resolve({
      ok: true,
      get value() { lateValueReads += 1; return result.value; },
    }), 5_300));
    const timeoutStarted = Date.now();
    await assert.rejects(engine.captureCurrentClipboard(), /UC_ENGINE:/);
    assert.ok(Date.now() - timeoutStarted >= 4_500, 'pending Host callback should reach its deadline');
    readOverride = undefined;
    assert.ok(await engine.captureCurrentClipboard(), 'next read must work after timeout');
    await new Promise((resolve) => setTimeout(resolve, 400));
    assert.equal(lateValueReads, 0, 'expired result must not copy clipboard data');

    readOverride = () => new Promise(() => {});
    await assert.rejects(engine.captureCurrentClipboard(), /UC_ENGINE:/);
    readOverride = undefined;
    assert.ok(await engine.captureCurrentClipboard(), 'never-resolved Promise must not block later operations');

    await engine.lockEncryption();
    assert.deepEqual(await engine.queryEncryptionState(), { initialized: true, sessionReady: false });
    assert.equal((await engine.recoverSession(false)).unlocked, false);
    await assert.rejects(engine.unlockSpace('synthetic wrong passphrase'), /UC_ENGINE:/);
    assert.equal((await engine.queryEncryptionState()).sessionReady, false);
    assert.equal(await engine.unlockSpace('correct horse battery staple'), created.spaceId);
    assert.equal((await engine.queryEncryptionState()).sessionReady, true);
    await engine.recoverSession(false);
    assert.equal((await engine.getHistoryEntry(report.entryId)).content, 'private HarmonyOS binding text');
    assert.equal((await engine.updateSyncSettings({ syncOnRestore: !originalSync.syncOnRestore })).kind, 'updated');
    const persistedSync = await engine.querySyncSettings();

    await engine.suspend();
    await waitForState(engine, 'suspended');
    await engine.resume();
    await waitForState(engine, 'running');
    await engine.shutdown(5_000);

    const reusedSetup = addon.installProcessObservability(observabilityConfig, hostDirectories);
    assert.equal(reusedSetup.reused, true);
    assert.throws(
      () => addon.installProcessObservability(
        { ...observabilityConfig, appChannel: 'development' },
        hostDirectories
      ),
      /OHOS_OBSERVABILITY_CONFIG_CONFLICT/
    );
    const restarted = await addon.startEngine(
      { appVersion: '1.2.3', profileId: 'ohos-host-smoke' },
      addon.prepareHost(host),
      addon.createStartupLifecycle()
    );
    await assert.rejects(
      restarted.createSpace('ohos-host-smoke', 'correct horse battery staple'),
      /UC_ENGINE:1205:conflict:false/
    );
    const recovery = await restarted.recoverSession(true);
    assert.equal(recovery.unlocked, true);
    assert.equal(recovery.resumed, true);

    assert.deepEqual(await restarted.querySyncSettings(), persistedSync);
    assert.equal((await restarted.querySetupState()).spaceId, created.spaceId);

    const restartText = 'private HarmonyOS restart export';
    const restartReport = await restarted.sendText(restartText, []);
    const restartOutput = requireFile(files, 'output-file-1');
    restartOutput.bytes = Buffer.alloc(0);
    restartOutput.finished = false;
    await restarted.exportEntry(restartReport.entryId, 'output-file-1');
    assert.deepEqual(restartOutput.bytes, Buffer.from(restartText));
    assert.equal(restartOutput.finished, true);
    await restarted.shutdown(5_000);

    values.clear();
    const restricted = await addon.startEngine(
      { appVersion: '1.2.3', profileId: 'ohos-host-smoke' },
      addon.prepareHost(host), addon.createStartupLifecycle()
    );
    const beforeRecovery = await restricted.queryProfileRecovery();
    assert.equal(beforeRecovery.state, 'awaiting_passphrase');
    assert.equal(beforeRecovery.canSubmitPassphrase, true);
    assert.equal(beforeRecovery.backgroundReady, false);
    assert.equal((await restricted.queryEncryptionState()).sessionReady, false);
    await assert.rejects(restricted.queryHistory(null, 10, null), /UC_ENGINE:/);
    await assert.rejects(restricted.getHistoryEntry(report.entryId), /UC_ENGINE:/);
    await assert.rejects(restricted.unlockSpace('synthetic wrong passphrase'), /UC_ENGINE:/);
    assert.equal(values.size, 0, 'wrong recovery passphrase must not write secure storage');
    assert.equal((await restricted.queryProfileRecovery()).canSubmitPassphrase, true);
    assert.equal(await restricted.unlockSpace('correct horse battery staple'), created.spaceId);
    const recoveredProfile = await restricted.queryProfileRecovery();
    assert.equal(recoveredProfile.state, 'recovered');
    assert.equal(recoveredProfile.backgroundReady, true);
    assert.deepEqual(recoveredProfile.losses, []);
    let profileEvent;
    const profileDeadline = Date.now() + 5_000;
    while (Date.now() < profileDeadline) {
      const event = await restricted.nextEvent(100);
      if (event?.kind === 'profile_recovery_changed' && event.profileRecovery?.state === 'recovered') {
        profileEvent = event;
        break;
      }
    }
    assert.deepEqual(profileEvent?.profileRecovery, recoveredProfile);
    assert.ok(values.size > 0);
    assert.equal((await restricted.getHistoryEntry(report.entryId)).content, 'private HarmonyOS binding text');
    assert.deepEqual(await restricted.querySyncSettings(), persistedSync);
    await restricted.shutdown(5_000);
    const shutdown = await addon.shutdownProcessObservability(1_000);
    assert.match(shutdown.traces, /^(completed|failed|timed_out)$/);
    assert.match(shutdown.logs, /^(completed|failed|timed_out)$/);
    const sensitiveMarkers = [
      Buffer.from(rejectedHostMarker),
      Buffer.from('synthetic wrong passphrase'),
      privateClipboardText,
      Buffer.from('private HarmonyOS binding text'),
      Buffer.from('synthetic observed clipboard content'),
      Buffer.from('correct horse battery staple'),
      Buffer.from(restartText),
    ];
    for (const relative of fs.readdirSync(root, { recursive: true })) {
      const file = path.join(root, relative);
      if (!fs.statSync(file).isFile()) continue;
      const bytes = fs.readFileSync(file);
      for (const marker of sensitiveMarkers) {
        assert.equal(bytes.includes(marker), false, 'private text and passphrase must not persist in plaintext');
      }
    }
    console.log('PASS: native content round-trip, history/search, observation, async Host success/rejection/timeout/late results, settings persistence, lock/unlock, lifecycle, restart, plaintext scan');
  } finally {
    await addon.shutdownProcessObservability(100).catch(() => {});
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function success(value) {
  return { ok: true, value };
}

function requireFile(files, handle) {
  const file = files.get(handle);
  assert.ok(file, `unknown file handle: ${handle}`);
  return file;
}

async function waitForState(engine, expected) {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    const event = await engine.nextEvent(100);
    if (event?.kind === 'state_changed' && event.state === expected) {
      return;
    }
  }
  assert.fail(`binding did not deliver state: ${expected}`);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
