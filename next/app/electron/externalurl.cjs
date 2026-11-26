'use strict';
// Which addresses Hyperlinks > Go To Destination may hand to the operating system's browser (user decision, 2026-10-28): only
// http:, https: and mailto:. file:, javascript:, data:, relative paths and empty values are never opened; the person is told instead.
// Pure and portable: the Electron main process asks it, and so does the test (next/oracle/linkgoto-ui-check.py).
const ALLOWED = ['http:', 'https:', 'mailto:'];

/** `{ ok: true, url }` (the normalised address to open) or `{ ok: false, reason: 'empty' | 'control' | 'relative' | 'scheme' }`. */
function checkExternalUrl(value) {
  if (typeof value !== 'string') return { ok: false, reason: 'empty' };
  const text = value.trim();
  if (!text) return { ok: false, reason: 'empty' };
  if (/[\u0000-\u001f\u007f]/.test(text)) return { ok: false, reason: 'control' };
  let url;
  try { url = new URL(text); } catch { return { ok: false, reason: 'relative' }; } // a relative path or a bare name has no scheme to parse
  if (!ALLOWED.includes(url.protocol)) return { ok: false, reason: 'scheme' };
  if (url.protocol !== 'mailto:' && !url.hostname) return { ok: false, reason: 'relative' };
  return { ok: true, url: url.href };
}

module.exports = { checkExternalUrl, ALLOWED };
