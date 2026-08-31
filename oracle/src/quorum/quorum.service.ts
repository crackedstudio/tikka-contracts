import { Contract, rpc as SorobanRpc, scValToNative, TransactionBuilder, Account, Keypair } from '@stellar/stellar-sdk';
import { randomBytes } from 'crypto';

export class QuorumService {
  constructor(
    private readonly rpcUrl: string,
    private readonly networkPassphrase: string,
    private readonly oracleAddress: string
  ) {}

  /**
   * Generates a cryptographically secure random u64 seed.
   */
  generateSecureSeed(): bigint {
    const bytes = randomBytes(8);
    return bytes.readBigUInt64BE(0);
  }

  /**
   * Queries the raffle contract's `get_raffle` view and checks if it's a Quorum raffle
   * that contains this oracle in its oracles list.
   */
  async checkQuorumParticipation(raffleContractId: string): Promise<{ isParticipant: boolean; k: number; oracles: string[] }> {
    const server = new SorobanRpc.Server(this.rpcUrl, { allowHttp: this.rpcUrl.startsWith('http://') });
    
    // Use a dummy source account to build the transaction for simulation.
    const dummySource = new Account(Keypair.random().publicKey(), '0');
    const contract = new Contract(raffleContractId);
    const tx = new TransactionBuilder(dummySource, {
      fee: '100000',
      networkPassphrase: this.networkPassphrase,
    })
      .addOperation(contract.call('get_raffle'))
      .setTimeout(30)
      .build();

    const simulated = await server.simulateTransaction(tx);
    if (SorobanRpc.Api.isSimulationError(simulated)) {
      throw new Error(`Failed to simulate get_raffle: ${JSON.stringify(simulated)}`);
    }
    if (!simulated.result?.retval) {
      throw new Error(`get_raffle returned empty result`);
    }
    
    const raffle = scValToNative(simulated.result.retval);
    const source = raffle.randomness_source;
    
    if (typeof source === 'object' && source !== null && 'Quorum' in source) {
      const quorum = source.Quorum;
      const k = Number(quorum.k);
      const oracles = quorum.oracles.map((addr: any) => addr.toString());
      const isParticipant = oracles.includes(this.oracleAddress);
      
      return { isParticipant, k, oracles };
    }

    return { isParticipant: false, k: 0, oracles: [] };
  }
}
