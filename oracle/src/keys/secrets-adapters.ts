/**
 * Adapter for loading secrets from Google Cloud Secret Manager.
 */
export class GcpSecretsAdapter {
  constructor(private readonly projectId: string) {}

  async getSecret(secretName: string): Promise<Buffer> {
    void this.projectId;
    void secretName;
    throw new Error('GCP Secret Manager adapter not fully implemented');
  }
}

/**
 * Adapter for loading secrets from HashiCorp Vault.
 */
export class VaultSecretsAdapter {
  constructor(
    private readonly vaultAddr: string,
    private readonly token: string = process.env.VAULT_TOKEN ?? ''
  ) {}

  async getSecret(secretPath: string): Promise<Buffer> {
    if (!this.token) {
      throw new Error('Vault token is required for VaultSecretsAdapter');
    }
    const response = await fetch(`${this.vaultAddr}/v1/${secretPath}`, {
      headers: {
        'X-Vault-Token': this.token,
      },
    });
    if (!response.ok) {
      throw new Error(`Failed to fetch secret from Vault: HTTP ${response.status}`);
    }
    const data = (await response.json()) as any;
    const value =
      data.data?.data?.value ??
      data.data?.value ??
      data.value ??
      data.data?.ORACLE_SECRET_KEY;
    if (!value) {
      throw new Error(`Secret not found in Vault payload at path: ${secretPath}`);
    }
    return Buffer.from(String(value));
  }
}

/**
 * Adapter for loading secrets from AWS KMS / Secrets Manager.
 */
export class AwsKmsSecretsAdapter {
  constructor(private readonly region: string = 'us-east-1') {}

  async getSecret(secretName: string): Promise<Buffer> {
    void this.region;
    void secretName;
    throw new Error('AWS KMS / Secrets Manager adapter not fully implemented');
  }
}

