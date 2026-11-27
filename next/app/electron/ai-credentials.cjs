const fs = require('node:fs/promises');
const path = require('node:path');
const { createHash, randomUUID } = require('node:crypto');

// The shell owns encryption. Only the selected engine receives a plaintext credential.
function credentials({ root, safeStorage, platform = process.platform }) {
  const memory = new Map();
  const directory = () => path.join(root(), 'ai', 'credentials');
  const file = id => path.join(directory(), createHash('sha256').update(id).digest('hex') + '.bin');
  function secure() {
    try {
      return safeStorage.isEncryptionAvailable() &&
        (platform !== 'linux' || ['gnome_libsecret', 'kwallet', 'kwallet5', 'kwallet6'].includes(safeStorage.getSelectedStorageBackend()));
    } catch { return false; }
  }
  async function erase(id) {
    await fs.rm(file(id), { force: true });
    memory.delete(id);
  }
  async function write(connection, key) {
    const { id, kind, baseUrl, agentProfile } = connection;
    if (!key) {
      await erase(id);
      return 'none';
    }
    const entry = JSON.stringify({ kind, baseUrl, agentProfile, key });
    let encrypted;
    if (secure()) {
      try { encrypted = safeStorage.encryptString(entry); } catch { /* Locked keyring. */ }
    }
    if (!encrypted) {
      await erase(id);
      memory.set(id, entry);
      return 'session';
    }
    await fs.mkdir(directory(), { recursive: true, mode: 0o700 });
    const target = file(id);
    const tmp = target + '.' + randomUUID();
    try {
      await fs.writeFile(tmp, encrypted, { flag: 'wx', mode: 0o600 });
      await fs.rename(tmp, target);
    } finally {
      await fs.rm(tmp, { force: true });
    }
    memory.delete(id);
    return 'secure';
  }
  async function read({ id, kind, baseUrl, agentProfile }) {
    let serialized = memory.get(id);
    if (!serialized) {
      if (!secure()) return '';
      let bytes;
      try { bytes = await fs.readFile(file(id)); }
      catch (e) { if (e.code === 'ENOENT') return ''; throw new Error('credential storage unavailable'); }
      try { serialized = safeStorage.decryptString(bytes); } catch { return ''; }
    }
    try {
      const entry = JSON.parse(serialized);
      return entry.kind === kind && entry.baseUrl === baseUrl && entry.agentProfile === agentProfile && typeof entry.key === 'string' ? entry.key : '';
    } catch { return ''; }
  }
  return { read, write, erase, secure };
}

function bridge(vault, raw, start, contexts) {
  let queue = Promise.resolve();
  const initialized = new WeakSet();
  async function handle(message, ctx) {
    if (!String(message.cmd).startsWith('ai')) return raw(message, ctx);
    const action = queue.then(async () => {
      if (!ctx.engine) start(ctx);
      const engine = ctx.engine;
      if (!engine) throw new Error('engine unavailable');
      if (!initialized.has(engine)) {
        const list = await raw({ cmd: 'aiConnections' }, ctx);
        if (!list.header.ok) return { reply: list };
        for (const c of list.header.connections) {
          const key = await vault.read(c);
          if (key) {
            const reply = await raw({ cmd: 'aiSetCredential', connectionId: c.id, kind: c.kind, baseUrl: c.baseUrl, agentProfile: c.agentProfile, key }, ctx);
            if (!reply.header.ok) return { reply };
          }
        }
        initialized.add(engine);
      }
      if (message.cmd === 'aiSaveConnection') {
        const list = await raw({ cmd: 'aiConnections' }, ctx);
        if (!list.header.ok) return { reply: list };
        const old = list.header.connections.find(c => c.id === message.connection?.id);
        const reply = await raw(message, ctx);
        if (reply.header.ok && old) {
          const updated = await raw({ cmd: 'aiConnections' }, ctx);
          const current = updated.header.connections?.find(c => c.id === old.id);
          if (!current || current.kind !== old.kind || current.baseUrl !== old.baseUrl || current.agentProfile !== old.agentProfile) {
            await vault.erase(old.id);
            for (const other of contexts.values()) {
              if (other.engine) await raw({ cmd: 'aiSetCredential', connectionId: old.id, key: '' }, other);
            }
          }
        }
        return { reply };
      }
      if (!['aiSetCredential', 'aiDeleteConnection'].includes(message.cmd)) {
        // Do not hold the vault queue while the model list is in flight.
        return { reply: raw(message, ctx) };
      }
      const id = message.connectionId;
      if (typeof id !== 'string' || !/^[A-Za-z0-9_-]{1,80}$/.test(id)) throw new Error('invalid connection ID');
      if (message.cmd === 'aiDeleteConnection') {
        await vault.erase(id);
        for (const other of contexts.values()) {
          if (other.engine) await raw({ cmd: 'aiSetCredential', connectionId: id, key: '' }, other);
        }
        return { reply: await raw(message, ctx) };
      }
      const key = message.key;
      if (typeof key !== 'string' || key.length > 8192 || /[^!-~]/.test(key)) throw new Error('invalid credential');
      const list = await raw({ cmd: 'aiConnections' }, ctx);
      const connection = list.header.connections?.find(c => c.id === id);
      if (!list.header.ok || !connection) {
        return { reply: { header: { ok: false, code: 'aiNotConfigured' }, body: new Uint8Array() } };
      }
      const storage = await vault.write(connection, key);
      const reply = await raw({ cmd: 'aiSetCredential', connectionId: id, kind: connection.kind, baseUrl: connection.baseUrl, agentProfile: connection.agentProfile, key }, ctx);
      for (const other of contexts.values()) {
        if (other !== ctx && other.engine && initialized.has(other.engine)) {
          await raw({ cmd: 'aiSetCredential', connectionId: id, kind: connection.kind, baseUrl: connection.baseUrl, agentProfile: connection.agentProfile, key }, other);
        }
      }
      reply.header.storage = storage;
      return { reply };
    });
    queue = action.catch(() => {});
    try { return await (await action).reply; }
    catch { throw new Error('AI credential or engine unavailable'); }
  }
  return handle;
}
// Diagnostic output accepts only a direction and the shell's numeric request ID.
function activity(direction, id) {
  return `AI ${direction === 'request' ? 'request' : 'reply'} id=${Number.isSafeInteger(id) ? id : '?'}`;
}
module.exports = { credentials, bridge, activity };
