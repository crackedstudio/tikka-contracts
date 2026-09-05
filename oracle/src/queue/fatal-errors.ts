/**
 * Contract-side error codes and message fragments that indicate a randomness
 * request can never succeed.  Matching entries are dead-lettered immediately
 * instead of retried.
 *
 * Soroban simulation failures embed the contract error code in the response;
 * we match both numeric codes and human-readable names.
 */
const FATAL_CONTRACT_ERRORS: ReadonlyArray<string | number> = [
  // Error::NoRandomnessRequest
  8,
  'NoRandomnessRequest',
  // Error::RandomnessAlreadyRequested
  7,
  'RandomnessAlreadyRequested',
  // Error::DrawingAlreadyComplete
  61,
  'DrawingAlreadyComplete',
  // Error::InvalidStatus (cancelled / failed raffle)
  23,
  'InvalidStatus',
  // Request ID mismatch and other permanent parameter failures
  21,
  'InvalidParameters',
  // Raffle never funded / prize not deposited
  11,
  'PrizeNotDeposited',
  // Oracle not configured for this raffle
  6,
  'OracleNotSet',
];

const REQUEST_ID_MISMATCH_FRAGMENTS = [
  'request_id',
  'request id',
  'RequestIdMismatch',
] as const;

/**
 * Returns true when `message` describes a permanent, non-retryable failure.
 */
export function isFatalRandomnessError(message: string): boolean {
  const lower = message.toLowerCase();

  for (const token of FATAL_CONTRACT_ERRORS) {
    if (typeof token === 'number') {
      // Soroban contract errors appear as "Error(Contract, #8)" or similar.
      if (
        message.includes(`#${token}`) ||
        message.includes(`Error(${token})`) ||
        message.includes(`error code ${token}`)
      ) {
        return true;
      }
    } else if (message.includes(token) || lower.includes(token.toLowerCase())) {
      return true;
    }
  }

  if (
    REQUEST_ID_MISMATCH_FRAGMENTS.some((fragment) => lower.includes(fragment)) &&
    (lower.includes('mismatch') || lower.includes('invalid'))
  ) {
    return true;
  }

  return false;
}
