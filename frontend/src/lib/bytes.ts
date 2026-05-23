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

// ---- Scalar decode -------------------------------------------------------

/** Mirrors `ScalarSpec` in `crates/exact-api/src/build.rs`. */
export type ScalarSpec =
  | 'u8'
  | 'i8'
  | 'u16_le'
  | 'u16_be'
  | 'i16_le'
  | 'i16_be'
  | 'u32_le'
  | 'u32_be'
  | 'i32_le'
  | 'i32_be'
  | 'u64_le'
  | 'u64_be'
  | 'i64_le'
  | 'i64_be';

export interface IoSpec {
  input: ScalarSpec;
  output: ScalarSpec;
}

const SCALAR_SIZE: Record<ScalarSpec, number> = {
  u8: 1,
  i8: 1,
  u16_le: 2,
  u16_be: 2,
  i16_le: 2,
  i16_be: 2,
  u32_le: 4,
  u32_be: 4,
  i32_le: 4,
  i32_be: 4,
  u64_le: 8,
  u64_be: 8,
  i64_le: 8,
  i64_be: 8
};

/** Decode `bytes` per `spec` and return a printable string. Returns null
 *  on length mismatch (caller can fall back to hex). */
export function decodeScalar(bytes: Uint8Array, spec: ScalarSpec): string | null {
  if (bytes.byteLength !== SCALAR_SIZE[spec]) return null;
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  switch (spec) {
    case 'u8':
      return String(bytes[0]);
    case 'i8':
      return String(dv.getInt8(0));
    case 'u16_le':
      return String(dv.getUint16(0, true));
    case 'u16_be':
      return String(dv.getUint16(0, false));
    case 'i16_le':
      return String(dv.getInt16(0, true));
    case 'i16_be':
      return String(dv.getInt16(0, false));
    case 'u32_le':
      return String(dv.getUint32(0, true));
    case 'u32_be':
      return String(dv.getUint32(0, false));
    case 'i32_le':
      return String(dv.getInt32(0, true));
    case 'i32_be':
      return String(dv.getInt32(0, false));
    case 'u64_le':
      return dv.getBigUint64(0, true).toString();
    case 'u64_be':
      return dv.getBigUint64(0, false).toString();
    case 'i64_le':
      return dv.getBigInt64(0, true).toString();
    case 'i64_be':
      return dv.getBigInt64(0, false).toString();
  }
}

/** Helper for UI: decode b64 → display string per spec, with hex fallback. */
export function decodeOutputB64(b64: string, spec: ScalarSpec | undefined): string {
  const bytes = b64ToBytes(b64);
  if (spec) {
    const decoded = decodeScalar(bytes, spec);
    if (decoded !== null) return decoded;
  }
  return bytesToHex(bytes);
}
