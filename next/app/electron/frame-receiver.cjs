// The engine sends a little-endian header length, UTF-8 JSON and an optional binary body.
const { constants } = require('node:buffer');
const empty = Buffer.alloc(0);

function receiverEnabled(override, preferences) {
  if (override === 'auto' || override === 'off') return override === 'auto';
  return preferences?.['folio.accel'] !== 'off';
}

function frameReceiver(deliver, enabled) {
  let header = null;
  let readingLength = true;
  let remaining = 4;
  let parts = [];
  return function receive(chunk) {
    let offset = 0;
    while (offset < chunk.length) {
      const count = Math.min(remaining, chunk.length - offset);
      const part = chunk.subarray(offset, offset + count);
      offset += count;
      remaining -= count;
      parts.push(part);
      // The compatibility path coalesces incomplete fields as their pieces arrive.
      if (remaining) {
        if (parts.length > 1 && !enabled()) parts = [Buffer.concat(parts)];
        return;
      }
      if (header) {
        // Give Electron an independent body, with no adjacent frame in the backing store.
        const body = Buffer.allocUnsafeSlow(header.bodyBytes);
        let written = 0;
        for (const bytes of parts) written += bytes.copy(body, written);
        deliver(header, body);
        header = null;
        readingLength = true;
        remaining = 4;
      } else {
        const bytes = parts.length === 1 ? part : Buffer.concat(parts);
        if (readingLength) {
          remaining = bytes.readUInt32LE(0);
          if (!remaining) throw Error('empty engine response header');
          readingLength = false;
        } else {
          header = JSON.parse(bytes.toString('utf8'));
          if (!header || typeof header !== 'object' || Array.isArray(header)) {
            throw Error('invalid engine response header');
          }
          remaining = header.bodyBytes ?? 0;
          if (!Number.isSafeInteger(remaining) || remaining < 0 || remaining > constants.MAX_LENGTH) {
            throw Error('invalid engine response body length');
          }
          if (!remaining) {
            deliver(header, empty);
            header = null;
            readingLength = true;
            remaining = 4;
          }
        }
      }
      parts = [];
    }
  };
}

module.exports = { frameReceiver, receiverEnabled };
