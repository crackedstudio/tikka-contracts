import { Keypair } from '@stellar/stellar-sdk';

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
      throw new Error(`Secret not found in environment: ${key}`);
    }
    return secret;
  }
}

/**
 * Adapter for loading secrets from AWS Secrets Manager in production.
 */
export class AwsSecretsAdapter implements SecretsAdapter {
  constructor(private readonly region: string = 'us-east-1') {}

  async getSecret(secretName: string): Promise<string> {
    // TODO: Implement actual AWS Secrets Manager fetch
    // import { SecretsManagerClient, GetSecretValueCommand } from "@aws-sdk/client-secrets-manager";
    // const client = new SecretsManagerClient({ region: this.region });
    // const response = await client.send(new GetSecretValueCommand({ SecretId: secretName }));
    // return response.SecretString;
    throw new Error('AWS Secrets Manager adapter not fully implemented');
  }
}

export class KeyService {
  private keypair!: Keypair;
  private initialized = false;

  constructor(
    private readonly adapter: SecretsAdapter = new EnvSecretsAdapter(),
    private readonly secretKeyName: string = 'ORACLE_SECRET_KEY'
  ) {}

  /**
   * Loads and validates the keypair from the configured secrets adapter.
   * Must be called at application startup.
   */
  async initialize(): Promise<void> {
    if (this.initialized) return;

    try {
      const secret = await this.adapter.getSecret(this.secretKeyName);
      this.keypair = Keypair.fromSecret(secret);
      this.initialized = true;
    } catch (error) {
      // Do not log the secret or full error details that might leak it
      console.error('Failed to initialize KeyService: Invalid or missing oracle secret key.');
      throw new Error('KeyService initialization failed.');
    }
  }

  /**
   * Retrieves the raw Keypair.
   */
  getKeypair(): Keypair {
    this.ensureInitialized();
    return this.keypair;
  }

  /**
   * Retrieves the public key (G...)
   */
  getPublicKey(): string {
    this.ensureInitialized();
    return this.keypair.publicKey();
  }

  /**
   * Signs a buffer of data with the oracle's secret key.
   */
  sign(data: Buffer): Buffer {
    this.ensureInitialized();
    return this.keypair.sign(data);
  }

  private ensureInitialized() {
    if (!this.initialized) {
      throw new Error('KeyService is not initialized. Call initialize() at startup.');
    }
  }
}
