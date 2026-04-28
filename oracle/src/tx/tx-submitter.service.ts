import { Transaction } from '@stellar/stellar-sdk';
import { KeyService } from '../keys/key.service';

export class TxSubmitterService {
  constructor(private readonly keyService: KeyService) {}

  /**
   * Example method to submit a transaction, utilizing the oracle's keypair for signing.
   */
  async submitTransaction(tx: Transaction): Promise<void> {
    const keypair = this.keyService.getKeypair();
    
    // Sign the transaction with the securely loaded keypair
    tx.sign(keypair);
    
    // Submit transaction logic goes here...
    console.log(`Transaction signed by oracle: ${keypair.publicKey()}`);
  }
}
