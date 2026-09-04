import { KeyService } from '../keys/key.service';
import { buildVrfProofMessage, deriveRandomSeedFromProof } from './proof-message';

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
  signRandomnessProof(raffleContract: string, requestId: bigint): RandomnessProof {
    const message = buildVrfProofMessage(raffleContract, requestId);
    const proof = this.keyService.sign(message);
    const randomSeed = deriveRandomSeedFromProof(proof);

    return {
      randomSeed,
      publicKey: this.keyService.getPublicKeyBytes(),
      proof: new Uint8Array(proof),
      requestId,
    };
  }
}
