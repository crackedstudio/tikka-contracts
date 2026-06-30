import { TxSubmitterService } from './tx-submitter.service';
import { KeyService } from '../keys/key.service';
import { buildVrfProofMessage } from '../vrf/proof-message';

/**
 * Integration test — skipped unless STELLAR_INTEGRATION_TEST=1 and env vars are set.
 * Run against testnet with a funded oracle account and deployed raffle contract.
 */
describe('TxSubmitterService integration', () => {
  const runIntegration = process.env.STELLAR_INTEGRATION_TEST === '1';

  (runIntegration ? it : it.skip)(
    'submits provide_randomness to testnet contract',
    async () => {
      const keyService = new KeyService();
      await keyService.initialize();

      const raffleContract = process.env.RAFFLE_CONTRACT_ADDRESS;
      const requestId = process.env.RANDOMNESS_REQUEST_ID;
      if (!raffleContract || !requestId) {
        throw new Error('RAFFLE_CONTRACT_ADDRESS and RANDOMNESS_REQUEST_ID required');
      }

      const randomSeed = BigInt(process.env.RANDOMNESS_SEED ?? '42');
      const message = buildVrfProofMessage(raffleContract, BigInt(requestId), randomSeed);
      const proof = keyService.sign(message);
      const publicKey = keyService.getPublicKeyBytes();

      const submitter = new TxSubmitterService(keyService);
      const hash = await submitter.submitProvideRandomness({
        raffleContract,
        randomSeed,
        publicKey: new Uint8Array(publicKey),
        proof: new Uint8Array(proof),
        requestId: BigInt(requestId),
      });

      expect(hash).toMatch(/^[a-f0-9]{64}$/);
    },
    120_000,
  );
});
