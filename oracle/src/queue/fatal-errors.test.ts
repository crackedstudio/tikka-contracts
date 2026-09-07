import { isFatalRandomnessError } from './fatal-errors';

describe('isFatalRandomnessError', () => {
  it('recognises contract error codes', () => {
    expect(isFatalRandomnessError('Simulation failed: Error(Contract, #8)')).toBe(true);
    expect(isFatalRandomnessError('contract trap: NoRandomnessRequest')).toBe(true);
    expect(isFatalRandomnessError('RandomnessAlreadyRequested')).toBe(true);
    expect(isFatalRandomnessError('DrawingAlreadyComplete')).toBe(true);
    expect(isFatalRandomnessError('InvalidStatus')).toBe(true);
    expect(isFatalRandomnessError('PrizeNotDeposited')).toBe(true);
  });

  it('recognises request id mismatch messages', () => {
    expect(isFatalRandomnessError('request_id mismatch: expected 1 got 2')).toBe(true);
    expect(isFatalRandomnessError('Invalid request id for this draw')).toBe(true);
  });

  it('does not classify transient RPC errors as fatal', () => {
    expect(isFatalRandomnessError('TxTooLate: transaction not confirmed')).toBe(false);
    expect(isFatalRandomnessError('ECONNRESET')).toBe(false);
    expect(isFatalRandomnessError('InsufficientFee')).toBe(false);
  });
});
