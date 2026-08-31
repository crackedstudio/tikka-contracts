import { Keypair } from '@stellar/stellar-sdk';
import { KeyService, EnvSecretsAdapter } from './key.service';
import { decodeSecretKey } from './secret-key';

describe('decodeSecretKey', () => {
  const keypair = Keypair.random();

  it('accepts Stellar secret keys', () => {
    const bytes = decodeSecretKey(keypair.secret());
    expect(bytes.length).toBe(32);
  });

  it('accepts 32-byte hex secrets', () => {
    const raw = Buffer.from(keypair.rawSecretKey());
    const bytes = decodeSecretKey(raw.toString('hex'));
    expect(bytes.equals(raw)).toBe(true);
  });

  it('rejects malformed secrets', () => {
    expect(() => decodeSecretKey('not-a-key')).toThrow('Invalid secret key format');
  });
});

describe('KeyService', () => {
  const keypair = Keypair.random();

  beforeEach(() => {
    process.env.ORACLE_SECRET_KEY = keypair.secret();
  });

  afterEach(() => {
    delete process.env.ORACLE_SECRET_KEY;
  });

  it('loads and validates the key on startup', async () => {
    const service = new KeyService();
    await service.initialize();
    expect(service.getPublicKey()).toBe(keypair.publicKey());
  });

  it('fails startup when the secret is missing', async () => {
    delete process.env.ORACLE_SECRET_KEY;
    const service = new KeyService();
    await expect(service.initialize()).rejects.toThrow('KeyService initialization failed.');
  });

  it('signs and verifies a message with the loaded key', async () => {
    const service = new KeyService();
    await service.initialize();

    const message = Buffer.from('oracle-proof-message');
    const signature = service.sign(message);

    expect(keypair.verify(message, signature)).toBe(true);
  });

  it('never exposes the secret in error messages', async () => {
    const service = new KeyService(new EnvSecretsAdapter(), 'MISSING_SECRET');
    await expect(service.initialize()).rejects.toThrow('KeyService initialization failed.');
  });

  it('zeroizes secret bytes on shutdown', async () => {
    const service = new KeyService();
    await service.initialize();
    service.shutdown();
    expect(() => service.sign(Buffer.from('x'))).toThrow('not initialized');
  });
});

import nock from 'nock';
import { VaultSecretsAdapter } from './key.service';

describe('VaultSecretsAdapter', () => {
  const vaultAddr = 'http://localhost:8200';
  const token = 's.mocktoken';

  it('fetches a secret from vault KV store', async () => {
    const adapter = new VaultSecretsAdapter(vaultAddr, token);
    const fetchSpy = jest.spyOn(global, 'fetch').mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        data: {
          data: {
            value: 'SAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4',
          },
        },
      }),
    } as any);

    const secret = await adapter.getSecret('secret/oracle');
    expect(secret.toString()).toBe('SAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4');
    expect(fetchSpy).toHaveBeenCalledWith(
      `${vaultAddr}/v1/secret/oracle`,
      expect.objectContaining({
        headers: { 'X-Vault-Token': token },
      })
    );
    fetchSpy.mockRestore();
  });

  it('throws when vault token is missing', async () => {
    const adapter = new VaultSecretsAdapter(vaultAddr, '');
    await expect(adapter.getSecret('secret/oracle')).rejects.toThrow('Vault token is required');
  });

  it('throws on non-200 responses', async () => {
    const adapter = new VaultSecretsAdapter(vaultAddr, token);
    const fetchSpy = jest.spyOn(global, 'fetch').mockResolvedValue({
      ok: false,
      status: 403,
      statusText: 'Forbidden',
    } as any);

    await expect(adapter.getSecret('secret/oracle')).rejects.toThrow('Failed to fetch secret from Vault');
    fetchSpy.mockRestore();
  });
});

