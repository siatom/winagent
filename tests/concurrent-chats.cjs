const { test } = require('node:test');
const assert = require('node:assert/strict');
const ts = require('typescript');
const fs = require('node:fs');
const vm = require('node:vm');

function setup() {
  const channels = new Map();
  const cancelled = [];
  const api = {
    getMessages: async () => [],
    sendMessage: async (id, text, channel) => { channels.set(id, channel); return `run-${id}`; },
    cancelRun: async (id) => cancelled.push(id),
  };
  const exports = {};
  const source = ts.transpileModule(fs.readFileSync('src/store/app.ts', 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, esModuleInterop: true },
  }).outputText;
  vm.runInNewContext(source, {
    exports, crypto: globalThis.crypto,
    require: (id) => id === '../lib/ipc' ? { api, runChannel: (callback) => callback }
      : id === '../i18n' ? { default: { t: (key) => key }, __esModule: true } : require(id),
  });
  const store = exports.useApp;
  const sessions = ['a', 'b', 'c'].map((id) => ({ id, title: id }));
  store.setState({ sessions, activeSessionId: 'a', settings: { maxConcurrentChats: 2 } });
  return { store, channels, cancelled, api };
}

test('parallel streams survive switching; stop and completion remain session scoped', async () => {
  const { store, channels, cancelled } = setup();
  await store.getState().send('first');
  channels.get('a')({ type: 'textDelta', delta: 'A' });
  await store.getState().selectSession('b');
  assert.equal(store.getState().streaming, false);
  await store.getState().send('second');
  channels.get('a')({ type: 'textDelta', delta: ' background' });
  channels.get('b')({ type: 'textDelta', delta: 'B' });
  assert.equal(store.getState().streamText, 'B');
  await store.getState().stop();
  assert.deepEqual(cancelled, ['run-b']);
  channels.get('a')({ type: 'finished', reason: 'stop' });
  assert.equal(store.getState().streaming, true);
  assert.equal(store.getState().streamText, 'B');
  assert.equal(store.getState().sessionStates.a.streaming, false);
});

test('limit blocks new runs and lowering it leaves existing runs intact', async () => {
  const { store, channels } = setup();
  await store.getState().send('first');
  await store.getState().selectSession('b');
  await store.getState().send('second');
  await store.getState().selectSession('c');
  await store.getState().send('third');
  assert.equal(channels.size, 2);
  assert.match(store.getState().error, /上限/);
  store.setState({ settings: { maxConcurrentChats: 1 } });
  assert.equal(store.getState().sessionStates.a.streaming, true);
  assert.equal(store.getState().sessionStates.b.streaming, true);
  channels.get('a')({ type: 'finished', reason: 'stop' });
  channels.get('b')({ type: 'finished', reason: 'stop' });
  await store.getState().send('retry');
  assert.equal(channels.size, 3);
});

test('approval queues and errors remain isolated between concurrent sessions', async () => {
  const { store, channels } = setup();
  await store.getState().send('first');
  await store.getState().selectSession('b');
  await store.getState().send('second');
  channels.get('a')({ type: 'approvalRequired', approvalId: 'approval-a' });
  channels.get('b')({ type: 'approvalRequired', approvalId: 'approval-b' });
  channels.get('a')({ type: 'failed', message: 'failed a' });
  assert.equal(store.getState().approvals.length, 1);
  assert.equal(store.getState().approvals[0].sessionId, 'b');
  assert.equal(store.getState().error, null);
  assert.equal(store.getState().streaming, true);
  await store.getState().selectSession('a');
  assert.equal(store.getState().error, 'failed a');
});

test('a slow history response cannot override a newer selection', async () => {
  const { store, api } = setup();
  let finish;
  api.getMessages = (id) => id === 'b' ? new Promise((resolve) => { finish = resolve; }) : Promise.resolve([]);
  const pending = store.getState().selectSession('b');
  await store.getState().selectSession('c');
  finish([]);
  await pending;
  assert.equal(store.getState().activeSessionId, 'c');
});

test('a draft reserves capacity while saving and can finish starting in the background', async () => {
  const { store, api, channels } = setup();
  let save;
  api.createSession = () => new Promise((resolve) => { save = resolve; });
  store.setState({ sessions: [{ id: 'draft-first', title: 'draft' }, { id: 'b', title: 'b' }], activeSessionId: 'draft-first', settings: { maxConcurrentChats: 1 } });
  const pending = store.getState().send('draft message');
  await store.getState().selectSession('b');
  await store.getState().send('blocked');
  assert.equal(channels.size, 0);
  save({ id: 'saved-first', title: 'first' });
  await pending;
  assert.equal(store.getState().activeSessionId, 'b');
  assert.equal(store.getState().messages.length, 0);
  assert.equal(store.getState().sessionStates['saved-first'].messages[0].content, 'draft message');
  await store.getState().selectSession('saved-first');
  assert.equal(store.getState().streaming, true);
  assert.equal(store.getState().messages.length, 1);
});
