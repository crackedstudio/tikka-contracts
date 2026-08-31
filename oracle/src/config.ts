import { Keypair } from '@stellar/stellar-sdk';
import { decodeSecretKey } from './keys/secret-key';

export interface OracleConfig {
  rpcUrl: string;
  factoryContractId: string;
  logLevel: string;
  pollIntervalMs: number;
  alertWebhookUrl: string;
  alertFailureThreshold: number;
  alertRateLimitMs: number;
  alertQueueDepthLimit: number;
  alertQueueAgeLimitMs: number;
  alertRpcUnreachableThreshold: number;
}

function readPositiveInt(name: string, defaultValue: number, errors: string[]): number {
  const raw = process.env[name];
  if (raw === undefined || raw.trim() === '') {
    return defaultValue;
  }

  const value = Number(raw);
  if (!Number.isFinite(value) || value <= 0) {
    errors.push(`${name} must be a positive number`);
    return defaultValue;
  }

  return Math.floor(value);
}

function isValidSecretKey(secret: string): boolean {
  const trimmed = secret.trim();

  // Accept Stellar S... secrets directly.
  if (trimmed.startsWith('S')) {
    try {
      Keypair.fromSecret(trimmed);
      return true;
    } catch {
      return false;
    }
  }

  // Reuse existing decoder support (hex/base64) and enforce 32-byte seed.
  try {
    const decoded = decodeSecretKey(trimmed);
    return decoded.length === 32;
  } catch {
    return false;
  }
}

export function loadAndValidateConfig(): OracleConfig {
  const errors: string[] = [];

  const rpcUrl = process.env.STELLAR_RPC_URL;
  if (!rpcUrl) {
    errors.push('STELLAR_RPC_URL is required');
  }

  const factoryContractId = process.env.FACTORY_CONTRACT_ID;
  if (!factoryContractId) {
    errors.push('FACTORY_CONTRACT_ID is required');
  }

  const rawPollInterval =
    process.env.POLL_INTERVAL_MS ?? process.env.ORACLE_POLL_INTERVAL_MS ?? '5000';
  const pollIntervalMs = Number(rawPollInterval);
  if (!Number.isFinite(pollIntervalMs) || pollIntervalMs <= 0) {
    errors.push('POLL_INTERVAL_MS must be a positive number');
  }

  const alertWebhookUrl = process.env.ALERT_WEBHOOK_URL ?? '';
  const alertFailureThreshold = readPositiveInt('ALERT_FAILURE_THRESHOLD', 3, errors);
  const alertRateLimitMs = readPositiveInt('ALERT_RATE_LIMIT_MS', 60_000, errors);
  const alertQueueDepthLimit = readPositiveInt('ALERT_QUEUE_DEPTH_LIMIT', 10, errors);
  const alertQueueAgeLimitMs = readPositiveInt('ALERT_QUEUE_AGE_LIMIT_MS', 300_000, errors);
  const alertRpcUnreachableThreshold = readPositiveInt('ALERT_RPC_UNREACHABLE_THRESHOLD', 3, errors);

  if (errors.length > 0) {
    console.error('Configuration errors:');
    for (const error of errors) {
      console.error(` - ${error}`);
    }
    process.exit(1);
  }

  return {
    rpcUrl: rpcUrl!,
    factoryContractId: factoryContractId!,
    logLevel: process.env.LOG_LEVEL ?? 'info',
    pollIntervalMs,
    alertWebhookUrl,
    alertFailureThreshold,
    alertRateLimitMs,
    alertQueueDepthLimit,
    alertQueueAgeLimitMs,
    alertRpcUnreachableThreshold,
  };
}
