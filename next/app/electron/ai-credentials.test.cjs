const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { createCipheriv, createDecipheriv, randomBytes } = require('node:crypto');
const { credentials, bridge } = require('./ai-credentials.cjs');

const connection = { id: 'one', kind: 'custom', baseUrl: 'https://example.invalid/v1' };
const key = 'test-key-not-real';

async function fixture(t) {
  const base = process.env.FOLIO_USER_DATA || path.resolve(__dirname, '../../../artifacts/ai-tests/userdata');
  await fs.mkdir(base, { recursive: true });
  const root = await fs.mkdtemp(path.join(base, 'vault-'));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  const encryptionKey = randomBytes(32);
  const safeStorage = {
    isEncryptionAvailable: () => true,
    getSelectedStorageBackend: () => 'gnome_libsecret',
    encryptString(s) {
      const iv = randomBytes(12);
      const cipher = createCipheriv('aes-256-gcm', encryptionKey, iv);
      const data = Buffer.concat([cipher.update(s, 'utf8'), cipher.final()]);
      return Buffer.concat([iv, cipher.getAuthTag(), data]);
    },
    decryptString(bytes) {
      const decipher = createDecipheriv('aes-256-gcm', encryptionKey, bytes.subarray(0, 12));
      decipher.setAuthTag(bytes.subarray(12, 28));
      return decipher.update(bytes.subarray(28), undefined, 'utf8') + decipher.final('utf8');
    },
  };
  const open = () => credentials({ root: () => root, platform: 'linux', safeStorage });
  return { root, open, safeStorage };
}

test('encrypted credentials survive restart only for the original endpoint', async t => {
  const { root, open } = await fixture(t);
  const vault = open();
  assert.equal(await vault.write(connection, key), 'secure');
  const dir = path.join(root, 'ai', 'credentials');
  const [file] = await fs.readdir(dir);
  assert.equal((await fs.readFile(path.join(dir, file))).includes(Buffer.from(key)), false);
  assert.equal(await open().read(connection), key);
  assert.equal(await open().read({ ...connection, baseUrl: 'https://elsewhere.invalid/v1' }), '');
  assert.equal(await open().read({ ...connection, kind: 'local' }), '');
  await vault.write(connection, key + '2');
  assert.equal(await open().read(connection), key + '2');
  await vault.erase(connection.id);
  assert.deepEqual(await fs.readdir(dir), []);
});

test('weak and unavailable OS storage retain credentials only for this process', async t => {
  const { root, open, safeStorage } = await fixture(t);
  const vault = open();
  await vault.write(connection, key);
  safeStorage.getSelectedStorageBackend = () => 'basic_text';
  assert.equal(await vault.write(connection, key + '2'), 'session');
  assert.equal(await vault.read(connection), key + '2');
  assert.equal(await open().read(connection), '');
  assert.deepEqual(await fs.readdir(path.join(root, 'ai', 'credentials')), []);
  safeStorage.isEncryptionAvailable = () => false;
  assert.equal(await vault.write(connection, key), 'session');
  await vault.erase(connection.id);
  assert.equal(await vault.read(connection), '');
});

test('a locked or replaced keyring still permits reconfiguration', async t => {
  const { open, safeStorage } = await fixture(t);
  const vault = open();
  await vault.write(connection, key);
  safeStorage.decryptString = () => { throw new Error('keyring replaced'); };
  assert.equal(await open().read(connection), '');
  safeStorage.encryptString = () => { throw new Error('keyring locked'); };
  assert.equal(await vault.write(connection, key), 'session');
  assert.equal(await vault.read(connection), key);
  assert.equal(await open().read(connection), '');
});

test('engine restart, rotation, endpoint edits and deletion affect every open window', async t => {
  const { open } = await fixture(t);
  const vault = open();
  let saved = { ...connection };
  const windows = new Map([[1, {}], [2, {}]]);
  const start = ctx => { ctx.engine = { key: '' }; };
  async function raw(message, ctx) {
    const header = { ok: true };
    if (message.cmd === 'aiConnections') header.connections = saved ? [{ ...saved, hasKey: !!ctx.engine.key }] : [];
    if (message.cmd === 'aiSetCredential') { ctx.engine.key = message.key; header.hasKey = !!message.key; }
    if (message.cmd === 'aiSaveConnection') saved = message.connection;
    if (message.cmd === 'aiDeleteConnection') saved = null;
    return { header, body: new Uint8Array() };
  }
  const invoke = bridge(vault, raw, start, windows);
  const [a, b] = windows.values();
  await invoke({ cmd: 'aiConnections' }, b);
  const set = value => invoke({ cmd: 'aiSetCredential', connectionId: 'one', key: value }, a);
  const answer = await set(key);
  assert.deepEqual(answer.header, { ok: true, hasKey: true, storage: 'secure' });
  assert.equal(b.engine.key, key);
  b.engine = null;
  await invoke({ cmd: 'aiConnections' }, b);
  assert.equal(b.engine.key, key);
  await set(key + '2');
  assert.equal(b.engine.key, key + '2');
  await invoke({ cmd: 'aiSaveConnection', connection: { ...connection, baseUrl: 'https://other.invalid/v1' } }, a);
  assert.equal(await vault.read(connection), '');
  assert.equal(b.engine.key, '');
  await set(key);
  await invoke({ cmd: 'aiDeleteConnection', connectionId: 'one' }, a);
  assert.equal(b.engine.key, '');
  assert.equal(a.engine.key, '');
  assert.equal(await vault.read({ ...connection, baseUrl: 'https://other.invalid/v1' }), '');
});

test('AI diagnostic lines contain only a direction and a numeric request ID', () => {
  const { activity } = require('./ai-credentials.cjs');
  assert.equal(activity('request', 12), 'AI request id=12');
  assert.equal(activity('test-key-not-real', 'test-key-not-real'), 'AI reply id=?');
});

test('ACP credentials belong to the saved agent launch profile', async t => {
  const { open } = await fixture(t);
  const agent = { id: 'agent', kind: 'acp', baseUrl: '', agentProfile: 'first-profile' };
  await open().write(agent, key);
  assert.equal(await open().read(agent), key);
  assert.equal(await open().read({ ...agent, agentProfile: 'different-profile' }), '');
});
