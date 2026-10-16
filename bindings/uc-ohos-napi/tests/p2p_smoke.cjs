const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { randomBytes } = require('node:crypto');
const { setTimeout: delay } = require('node:timers/promises');
const { assertDeclaredMethods } = require('./declaration_contract.cjs');

function memoryHost(root) {
  const secrets = new Map();
  const files = new Map();
  const fileNames = new Map();
  let clipboard = { observedAtMs: Date.now(), representations: [] };
  const success = value => ({ ok: true, value });
  const host = {
    privateDataDirectory: path.join(root, 'data'),
    cacheDirectory: path.join(root, 'cache'),
    temporaryDirectory: path.join(root, 'temporary'),
    secureStorageGet: key => success(secrets.has(key) ? new Uint8Array(secrets.get(key)) : null),
    secureStorageSet: (key, bytes) => { secrets.set(key, Buffer.from(bytes)); return success(); },
    secureStorageDelete: key => { secrets.delete(key); return success(); },
    clipboardRead: () => success(clipboard),
    clipboardWrite: snapshot => { clipboard = snapshot; return success(); },
    fileMetadata: handle => success({ displayName: fileNames.get(handle) ?? 'fixture.txt', sizeBytes: String(files.get(handle).length), mimeType: 'text/plain' }),
    fileReadChunk: (handle, offset, count) => success(new Uint8Array(files.get(handle).subarray(Number(offset), Number(offset) + count))),
    fileWriteChunk: (handle, offset, bytes) => {
      const previous = files.get(handle) ?? Buffer.alloc(0);
      const target = Buffer.alloc(Math.max(previous.length, Number(offset) + bytes.length));
      previous.copy(target);
      Buffer.from(bytes).copy(target, Number(offset));
      files.set(handle, target);
      return success();
    },
    fileFinishWrite: () => success()
  };
  return {
    host, files, fileNames, clipboard: () => clipboard,
    setClipboardText(text) {
      clipboard = {
        observedAtMs: Date.now(),
        representations: [{ kind: 'inline', format: 'text/plain', mimeType: 'text/plain', bytes: Buffer.from(text) }]
      };
    }
  };
}

async function until(predicate, label, budget = 30000) {
  const deadline = Date.now() + budget;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await delay(100);
  }
  throw new Error(`Timed out: ${label}`);
}


async function worker(root) {
  const addon = require(process.env.UC_OHOS_NAPI_NODE);
  assertDeclaredMethods(addon);
  const fixture = memoryHost(root);
  addon.installProcessObservability({ serviceVersion: '0.1.0', environment: 'test', appChannel: 'test', remoteDiagnosticsEnabled: false }, fixture.host);
  const startup = addon.startEngine({ appVersion: '0.1.0', profileId: 'p2p-fixture' }, addon.prepareHost(fixture.host), addon.createStartupLifecycle());
  const collecting = typeof global.gc === 'function' ? setInterval(() => global.gc(), 1) : undefined;
  let engine;
  try { engine = await startup; }
  finally { if (collecting !== undefined) clearInterval(collecting); }
  process.on('message', async ({ id, method, args }) => {
    try {
      let value;
      if (method === 'clipboardText') {
        value = fixture.clipboard().representations.filter(item => item.bytes).map(item => Buffer.from(item.bytes).toString());
      } else if (method === 'clipboardBytes') {
        value = fixture.clipboard().representations.filter(item => item.bytes).map(item => Buffer.from(item.bytes));
      } else if (method === 'setClipboardText') {
        fixture.setClipboardText(args[0]);
      } else if (method === 'putFile') {
        fixture.files.set(args[0], Buffer.from(args[1]));
        if (args[2] !== undefined) fixture.fileNames.set(args[0], args[2]);
      } else if (method === 'getFile') {
        value = fixture.files.get(args[0]);
      } else if (method === 'drainEvents') {
        value = [];
        for (let index = 0; index < 256; index++) {
          const event = await engine.nextEvent(1);
          if (event === null) break;
          value.push(event);
        }
      } else {
        value = await engine[method](...args);
      }
      if (method === 'shutdownUntilComplete') await addon.shutdownProcessObservability(1000);
      process.send({ id, value });
    } catch (error) {
      process.send({ id, error: error.message });
    }
  });
  process.send({ ready: true });
}

async function startPeer(root) {
  const { fork } = require('node:child_process');
  const child = fork(__filename, ['--worker', root], { stdio: ['ignore', 'inherit', 'inherit', 'ipc'], serialization: 'advanced' });
  const pending = new Map();
  let sequence = 0;
  let readyResolve;
  let readyReject;
  const ready = new Promise((resolve, reject) => { readyResolve = resolve; readyReject = reject; });
  const startupTimer = setTimeout(() => readyReject(new Error('Peer startup timed out')), 30000);
  child.on('message', message => {
    if (message.ready) return readyResolve();
    if (message.startupError) return readyReject(new Error(message.startupError));
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    clearTimeout(request.timer);
    if (message.error) request.reject(new Error(message.error));
    else request.resolve(message.value);
  });
  child.on('error', readyReject);
  child.on('exit', (code, signal) => {
    const error = new Error(`Native peer exited: code=${code}, signal=${signal}`);
    readyReject(error);
    for (const request of pending.values()) {
      clearTimeout(request.timer);
      request.reject(error);
    }
    pending.clear();
  });
  const call = (method, ...args) => new Promise((resolve, reject) => {
    if (!child.connected) return reject(new Error('Native peer disconnected'));
    const id = ++sequence;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`Native operation timed out: ${method}`)); }, 30000);
    pending.set(id, { resolve, reject, timer });
    child.send({ id, method, args });
  });
  try { await ready; } catch (error) { child.kill(); throw error; }
  finally { clearTimeout(startupTimer); }
  return {
    call,
    async stop() {
      try { await call('shutdownUntilComplete'); }
      finally {
        if (child.exitCode === null && child.signalCode === null) {
          const exited = new Promise(resolve => child.once('exit', resolve));
          child.kill();
          await exited;
        }
      }
    }
  };
}

async function main() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'uc-ohos-p2p-'));
  const peers = [];
  try {
    const left = await startPeer(path.join(root, 'a')); peers.push(left);
    const right = await startPeer(path.join(root, 'b')); peers.push(right);
    const passphrase = 'synthetic pairing fixture passphrase';
    console.log('stage: create space');
    await left.call('createSpace', 'Fixture A', passphrase);
    console.log('stage: issue invitation');
    const invitation = await left.call('issueInvitation');
    console.log('stage: join space');
    const join = await right.call('joinSpace', invitation.fullInvitation, 'Fixture B', passphrase, false);
    assert.ok(['active', 'pending', 'processing'].includes(join.status), 'pairing must enter an admissible state');
    console.log('stage: wait for roster');
    await until(async () => (await left.call('listDevices')).length === 2 && (await right.call('listDevices')).length === 2, 'two-member roster');
    const leftId = (await left.call('queryLocalDevice')).deviceId;
    const rightId = (await right.call('queryLocalDevice')).deviceId;
    const preferences = await left.call('queryMemberSyncPreferences', rightId);
    const updated = await left.call('updateMemberSyncPreferences', rightId, { ...preferences, sendContentTypes: { ...preferences.sendContentTypes, image: false } });
    assert.equal(updated.sendContentTypes.image, false);
    await left.call('updateMemberSyncPreferences', rightId, preferences);

    await left.call('sendText', 'p2p fixture left to right', [rightId]);
    await until(async () => (await right.call('clipboardText')).includes('p2p fixture left to right'), 'left to right clipboard');
    console.log('stage: global sync controls');
    const originalSync = await left.call('querySyncSettings');
    for (const patch of [
      { syncEnabled: false, autoSyncEnabled: true },
      { syncEnabled: true, autoSyncEnabled: false }
    ]) {
      const remoteBefore = await right.call('queryHistory', null, 100, null);
      const remoteClipboard = await right.call('clipboardText');
      assert.equal((await left.call('updateSyncSettings', patch)).kind, 'updated');
      const text = `p2p paused observation ${patch.syncEnabled}`;
      await left.call('setClipboardText', text);
      await assert.rejects(left.call('observeClipboardChange', true), /^Error: UC_ENGINE:1253:conflict:false$/);
      if (!patch.syncEnabled) {
        await assert.rejects(left.call('sendText', 'blocked explicit text', [rightId]), /^Error: UC_ENGINE:1253:conflict:false$/);
      }
      const local = await left.call('queryHistory', null, 100, null);
      let captured = false;
      for (const entry of local.entries) {
        if ((await left.call('getHistoryEntry', entry.entryId)).content === text) captured = true;
      }
      assert.equal(captured, true, 'disabled synchronization must retain local capture');
      assert.deepEqual(await right.call('clipboardText'), remoteClipboard);
      assert.deepEqual((await right.call('queryHistory', null, 100, null)).entries, remoteBefore.entries);
    }
    assert.equal((await left.call('updateSyncSettings', { syncEnabled: true, autoSyncEnabled: true })).kind, 'updated');
    await left.call('setClipboardText', 'p2p resumed observation');
    const observed = await left.call('observeClipboardChange', true);
    assert.ok(observed.entryId);
    await until(async () => (await right.call('clipboardText')).includes('p2p resumed observation'), 'resumed clipboard observation');
    assert.equal((await left.call('updateSyncSettings', originalSync)).kind, 'updated');

    const rightSync = await right.call('querySyncSettings');
    assert.equal((await right.call('updateSyncSettings', { autoSyncEnabled: false })).kind, 'updated');
    await right.call('sendText', 'p2p fixture right to left', [leftId]);
    await until(async () => (await left.call('clipboardText')).includes('p2p fixture right to left'), 'right to left clipboard');
    const before = await left.call('queryHistory', null, 100, null);
    await left.call('observeClipboardChange', true);
    const after = await left.call('queryHistory', null, 100, null);
    assert.equal(after.entries.length, before.entries.length, 'received clipboard write must not loop');

    console.log('stage: transfer image');
    assert.equal((await left.call('updateSyncSettings', { autoSyncEnabled: false })).kind, 'updated');
    const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aZXsAAAAASUVORK5CYII=', 'base64');
    await left.call('sendImage', png, 'image/png', [rightId]);
    await until(async () => (await right.call('clipboardBytes')).some(bytes => png.equals(bytes)), 'image bytes');

    console.log('stage: transfer file');
    const body = randomBytes(8 * 1024 * 1024);
    await right.call('putFile', 'fixture-input', body, '较大 文件.bin');
    await right.call('sendFiles', ['fixture-input'], [leftId]);
    let receivedFile;
    await until(async () => {
      receivedFile = (await left.call('queryHistory', null, 100, null)).entries.find(entry => entry.contentType === 'file');
      return receivedFile !== undefined;
    }, 'received file history');
    await until(async () => {
      try {
        await left.call('exportEntry', receivedFile.entryId, 'fixture-output');
        return body.equals(await left.call('getFile', 'fixture-output'));
      } catch (error) {
        if (!/^UC_ENGINE:/.test(error.message)) throw error;
        return false;
      }
    }, 'file body round-trip');

    console.log('stage: transfer empty file');
    const beforeEmpty = new Set((await left.call('queryHistory', null, 100, null)).entries.map(entry => entry.entryId));
    await right.call('putFile', 'empty-input', Buffer.alloc(0), '空 文件.txt');
    await right.call('sendFiles', ['empty-input'], [leftId]);
    let receivedEmpty;
    await until(async () => {
      receivedEmpty = (await left.call('queryHistory', null, 100, null)).entries.find(entry =>
        entry.contentType === 'file' && !beforeEmpty.has(entry.entryId));
      return receivedEmpty !== undefined;
    }, 'empty file history');
    await left.call('putFile', 'empty-output', Buffer.alloc(0));
    await until(async () => {
      try {
        await left.call('exportEntry', receivedEmpty.entryId, 'empty-output');
        return (await left.call('getFile', 'empty-output')).length === 0;
      } catch (error) {
        if (!/^UC_ENGINE:/.test(error.message)) throw error;
        return false;
      }
    }, 'empty file export');
    console.log('stage: transfer complete file collection and export ZIP');
    const beforeCollection = new Set((await left.call('queryHistory', null, 100, null)).entries.map(entry => entry.entryId));
    await right.call('putFile', 'second-input', Buffer.from('second member'), '较大 文件.bin');
    await right.call('sendFiles', ['fixture-input', 'second-input', 'empty-input'], [leftId]);
    let collection;
    await until(async () => {
      collection = (await left.call('queryHistory', null, 100, null)).entries.find(entry =>
        entry.contentType === 'file' && !beforeCollection.has(entry.entryId));
      return collection !== undefined;
    }, 'collection history');
    await until(async () => {
      try {
        await left.call('putFile', 'archive-output', Buffer.alloc(0));
        await left.call('exportEntryFiles', collection.entryId, 'archive-output');
        return true;
      } catch (error) {
        if (!/^UC_ENGINE:/.test(error.message)) throw error;
        return false;
      }
    }, 'collection ZIP export');
    const archive = await left.call('getFile', 'archive-output');
    const digest = require('node:child_process').execFileSync('python3', ['-c',
      'import sys,io,zipfile,json,hashlib; z=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read())); assert z.testzip() is None; print(json.dumps({n:hashlib.sha256(z.read(n)).hexdigest() for n in z.namelist()}))'
    ], { input: archive, encoding: 'utf8' });
    const hash = bytes => require('node:crypto').createHash('sha256').update(bytes).digest('hex');
    assert.deepEqual(JSON.parse(digest), {
      '较大 文件.bin': hash(body),
      '较大 文件 (2).bin': hash(Buffer.from('second member')),
      '空 文件.txt': hash(Buffer.alloc(0))
    });

    const transferEvents = [...await left.call('drainEvents'), ...await right.call('drainEvents')];
    const progress = transferEvents.filter(event => event.kind === 'transfer_progress');
    assert.ok(progress.length > 0, 'real transfers must deliver progress events');
    for (const event of progress) {
      assert.ok(event.transferProgress.transferId);
      assert.match(event.transferProgress.completedBytes, /^\d+$/);
      assert.ok(['sending', 'receiving'].includes(event.transferProgress.direction));
    }

    assert.equal((await left.call('updateSyncSettings', originalSync)).kind, 'updated');
    assert.equal((await right.call('updateSyncSettings', rightSync)).kind, 'updated');

    const result = JSON.parse(await left.call('removeMember', rightId));
    assert.equal(typeof result, 'object', 'successful removal must return current trust state');
    assert.ok(!Array.isArray(result));
    assert.equal((await left.call('listDevices')).filter(item => item.deviceId === rightId).length, 0);
    console.log('PASS: native pairing, bidirectional text, image/file bytes, preferences, global sync controls, loop suppression, member removal');
  } finally {
    for (const peer of peers.reverse()) await peer.stop().catch(error => { console.error(error); process.exitCode = 1; });
    fs.rmSync(root, { recursive: true, force: true });
  }
}

if (process.argv[2] === '--worker') {
  worker(process.argv[3]).catch(error => {
    process.send({ startupError: error.message }, () => process.exit(1));
  });
} else {
  main().catch(error => { console.error(error); process.exitCode = 1; });
}
