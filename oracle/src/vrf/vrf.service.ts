import { KeyService } from '../keys/key.service';
import { buildVrfProofMessage } from './proof-message';

export interface RandomnessProof {
  randomSeed: bigint;
  publicKey: Uint8Array;
  proof: Uint8Array;
  requestId: bigint;
}

export class VrfService {
  constructor(private readonly keyService: KeyService) {}

  /**
   * Signs a randomness reveal bound to a specific raffle contract and request.
   */
  signRandomnessProof(
    raffleContract: string,
    requestId: bigint,
    randomSeed: bigint,
  ): RandomnessProof {
    const message = buildVrfProofMessage(raffleContract, requestId, randomSeed);
    const proof = this.keyService.sign(message);

    return {
      randomSeed,
      publicKey: this.keyService.getPublicKeyBytes(),
      proof: new Uint8Array(proof),
      requestId,
    };
  }
}
