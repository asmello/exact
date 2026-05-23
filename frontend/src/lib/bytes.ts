// Helpers for shuttling raw bytes through forms + the JSON API.
// Test case inputs/outputs ride as base64 over the wire; admin forms accept
// hex (whitespace + optional 0x prefix tolerated) since that's the format
// most engineers reach for when inspecting byte payloads.

export function hexToBytes(s: string): Uint8Array {
  const clean = s.replace(/\s+/g, '').replace(/^0x/i, '');
  if (clean.length === 0) return new Uint8Array(0);
  if (clean.length % 2 !== 0) throw new Error('hex must have even length');
  if (!/^[0-9a-fA-F]+$/.test(clean)) throw new Error('hex contains non-hex chars');
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.substr(i * 2, 2), 16);
  return out;
}

export function bytesToHex(b: Uint8Array): string {
  return Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
}

export function bytesToB64(b: Uint8Array): string {
  let s = '';
  for (const x of b) s += String.fromCharCode(x);
  return btoa(s);
}

export function b64ToBytes(s: string): Uint8Array {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function hexToB64(hex: string): string {
  return bytesToB64(hexToBytes(hex));
}

export function b64ToHex(b64: string): string {
  return bytesToHex(b64ToBytes(b64));
}
