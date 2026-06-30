import { Keypair } from '@stellar/stellar-sdk';
import { KeyService } from '../keys/key.service';
import { VrfService } from './vrf.service';
import { buildVrfProofMessage } from './proof-message';

describe('buildVrfProofMessage', () => {
  it('produces distinct messages for different raffle contracts', () => {
    const requestId = 42n;
    const seed = 99n;

    const messageA = buildVrfProofMessage(
      'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4',
      requestId,
      seed,
    );
    const messageB = buildVrfProofMessage(
      'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM',
      requestId,
      seed,
    );

    expect(messageA.equals(messageB)).toBe(false);
  });
});

describe('VrfService', () => {
  const keypair = Keypair.random();

  beforeEach(() => {
    process.env.ORACLE_SECRET_KEY = keypair.secret();
  });

  afterEach(() => {
    delete process.env.ORACLE_SECRET_KEY;
  });

  it('signs and verifies a context-bound randomness proof', async () => {
    const keyService = new KeyService();
    await keyService.initialize();

    const vrf = new VrfService(keyService);
    const raffleContract = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4';
    const requestId = 7n;
    const randomSeed = 123456789n;

    const signed = vrf.signRandomnessProof(raffleContract, requestId, randomSeed);
    const message = buildVrfProofMessage(raffleContract, requestId, randomSeed);

    expect(signed.publicKey).toEqual(keyService.getPublicKeyBytes());
    expect(keypair.verify(message, Buffer.from(signed.proof))).toBe(true);
  });
});
