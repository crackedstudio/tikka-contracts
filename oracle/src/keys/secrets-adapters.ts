/**
 * Adapter for loading secrets from Google Cloud Secret Manager.
 */
export class GcpSecretsAdapter {
  constructor(private readonly projectId: string) {}

  async getSecret(secretName: string): Promise<string> {
    void this.projectId;
    void secretName;
    throw new Error('GCP Secret Manager adapter not fully implemented');
  }
}

/**
 * Adapter for loading secrets from HashiCorp Vault.
 */
export class VaultSecretsAdapter {
  constructor(private readonly vaultAddr: string) {}

  async getSecret(secretPath: string): Promise<string> {
    void this.vaultAddr;
    void secretPath;
    throw new Error('HashiCorp Vault adapter not fully implemented');
  }
}

/**
 * Adapter for loading secrets from AWS KMS / Secrets Manager.
 */
export class AwsKmsSecretsAdapter {
  constructor(private readonly region: string = 'us-east-1') {}

  async getSecret(secretName: string): Promise<string> {
    void this.region;
    void secretName;
    throw new Error('AWS KMS / Secrets Manager adapter not fully implemented');
  }
}
