const assert = require('node:assert/strict');
const test = require('node:test');
const { frameReceiver, receiverEnabled } = require('../electron/frame-receiver.cjs');

function frame(header, body = Buffer.alloc(0)) {
  const json = Buffer.from(JSON.stringify(header));
  const prefix = Buffer.alloc(4);
  prefix.writeUInt32LE(json.length);
  return Buffer.concat([prefix, json, body]);
}

test('the process override takes priority over persisted engine preferences', () => {
  assert.equal(receiverEnabled(undefined, undefined), true);
  assert.equal(receiverEnabled('off', { 'folio.accel': 'auto' }), false);
  assert.equal(receiverEnabled('auto', { 'folio.accel': 'off' }), true);
  assert.equal(receiverEnabled('unknown', { 'folio.accel': 'off' }), false);
});

for (const enabled of [true, false]) {
  test(`UTF-8, events and successive frames at every split (accelerated=${enabled})`, () => {
    const body = Buffer.from([0, 128, 255, 7]);
    const rows = [[{ event: '한글' }, Buffer.alloc(0)], [{ id: 1, bodyBytes: 4 }, body],
      [{ id: 2, bodyBytes: 0 }, Buffer.alloc(0)]];
    const wire = Buffer.concat(rows.map(([header, bytes]) => frame(header, bytes)));
    for (let cut = 0; cut <= wire.length; cut++) {
      const output = [];
      const input = Buffer.from(wire);
      const receive = frameReceiver((header, bytes) => output.push([header, bytes]), () => enabled);
      receive(input.subarray(0, cut));
      receive(input.subarray(cut));
      input.fill(0);
      assert.deepEqual(output, rows);
      assert.equal(output[1][1].buffer.byteLength, body.length);
    }
  });

  test(`incomplete length, header and body never dispatch (accelerated=${enabled})`, () => {
    const wire = frame({ id: 1, bodyBytes: 4 }, Buffer.alloc(4));
    for (let end = 0; end < wire.length; end++) {
      const receive = frameReceiver(() => assert.fail('premature response'), () => enabled);
      for (let i = 0; i < end; ++i) receive(wire.subarray(i, i + 1));
    }
    const receive = frameReceiver(() => assert.fail('invalid response'), () => enabled);
    assert.throws(() => receive(frame({ bodyBytes: -1 })), /invalid engine response body length/);
    for (const header of [null, 1, []]) {
      const invalid = frameReceiver(() => assert.fail('invalid response'), () => enabled);
      assert.throws(() => invalid(frame(header)), /invalid engine response header/);
    }
  });

  test(`400% page body across pipe chunks (accelerated=${enabled})`, () => {
    const bytes = Buffer.alloc(6912000);
    for (let i = 0; i < bytes.length; ++i) bytes[i] = i % 251;
    const wire = frame({ id: 1, bodyBytes: bytes.length }, bytes);
    let result;
    const receive = frameReceiver((_, body) => { result = body; }, () => enabled);
    for (let i = 0; i < wire.length; i += 65536) receive(wire.subarray(i, i + 65536));
    assert.deepEqual(result, bytes);
  });
}
