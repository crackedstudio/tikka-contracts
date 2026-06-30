import { Keypair } from '@stellar/stellar-sdk';

const HEX_SECRET_RE = /^(?:0x)?[0-9a-fA-F]{64}$/;
const BASE64_SECRET_RE = /^[A-Za-z0-9+/]{43}=$/;

/**
 * Decodes an oracle secret from Stellar secret key, 32-byte hex, or base64.
 */
export function decodeSecretKey(rawSecret: string): Buffer {
  const trimmed = rawSecret.trim();

  if (trimmed.startsWith('S')) {
    return Buffer.from(Keypair.fromSecret(trimmed).rawSecretKey());
  }

  if (HEX_SECRET_RE.test(trimmed)) {
    const hex = trimmed.startsWith('0x') ? trimmed.slice(2) : trimmed;
    const bytes = Buffer.from(hex, 'hex');
    if (bytes.length !== 32) {
      throw new Error('Invalid secret key format');
    }
    return bytes;
  }

  if (BASE64_SECRET_RE.test(trimmed) || trimmed.length >= 43) {
    try {
      const bytes = Buffer.from(trimmed, 'base64');
      if (bytes.length === 32) {
        return bytes;
      }
    } catch {
      // fall through to generic error below
    }
  }

  throw new Error('Invalid secret key format');
}

export function zeroizeBuffer(buffer: Buffer): void {
  buffer.fill(0);
}
