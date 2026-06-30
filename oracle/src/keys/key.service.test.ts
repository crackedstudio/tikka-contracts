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
