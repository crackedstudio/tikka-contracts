import { Keypair } from '@stellar/stellar-sdk';
import { decodeSecretKey, zeroizeBuffer } from './secret-key';

export interface SecretsAdapter {
  getSecret(key: string): Promise<string>;
}

/**
 * Adapter for loading secrets from environment variables.
 */
export class EnvSecretsAdapter implements SecretsAdapter {
  async getSecret(key: string): Promise<string> {
    const secret = process.env[key];
    if (!secret) {
      throw new Error('ORACLE_SECRET_KEY env var not set');
    }
    return secret;
  }
}

export class KeyService {
  private keypair!: Keypair;
  private secretBytes?: Buffer;
  private initialized = false;

  constructor(
    private readonly adapter: SecretsAdapter = new EnvSecretsAdapter(),
    private readonly secretKeyName: string = 'ORACLE_SECRET_KEY',
  ) {}

  /**
   * Loads and validates the keypair from the configured secrets adapter.
   * Must be called at application startup.
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      return;
    }

    try {
      const rawSecret = await this.adapter.getSecret(this.secretKeyName);
      this.secretBytes = decodeSecretKey(rawSecret);
      this.keypair = Keypair.fromRawEd25519Seed(this.secretBytes);
      this.initialized = true;
    } catch {
      console.error('Failed to initialize KeyService: Invalid or missing oracle secret key.');
      throw new Error('KeyService initialization failed.');
    }
  }

  getKeypair(): Keypair {
    this.ensureInitialized();
    return this.keypair;
  }

  getPublicKey(): string {
    this.ensureInitialized();
    return this.keypair.publicKey();
  }

  getPublicKeyBytes(): Uint8Array {
    this.ensureInitialized();
    return new Uint8Array(this.keypair.rawPublicKey());
  }

  sign(data: Buffer): Buffer {
    this.ensureInitialized();
    return this.keypair.sign(data);
  }

  /**
   * Zeroizes private key material from memory. Call on shutdown.
   */
  shutdown(): void {
    if (this.secretBytes) {
      zeroizeBuffer(this.secretBytes);
      this.secretBytes = undefined;
    }
    this.initialized = false;
  }

  private ensureInitialized(): void {
    if (!this.initialized) {
      throw new Error('KeyService is not initialized. Call initialize() at startup.');
    }
  }
}

export { AwsKmsSecretsAdapter, GcpSecretsAdapter, VaultSecretsAdapter } from './secrets-adapters';
export { decodeSecretKey } from './secret-key';
