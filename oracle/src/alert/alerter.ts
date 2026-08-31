export type AlertSeverity = 'info' | 'warning' | 'critical';

export interface AlertPayload {
  type: string;
  severity: AlertSeverity;
  message: string;
  timestamp: number;
  details?: Record<string, unknown>;
}

export interface AlerterOptions {
  webhookUrl?: string;
  rateLimitMs?: number;
  fetchImpl?: typeof fetch;
}

/**
 * Delivers operational alerts to a generic JSON webhook (Slack / Discord /
 * PagerDuty compatible). Alerts are rate-limited per type so repeated failures
 * collapse into a single aggregated notification within the cooldown window.
 */
export class Alerter {
  private readonly webhookUrl: string;
  private readonly rateLimitMs: number;
  private readonly fetchImpl: typeof fetch;
  private readonly lastSentAt: Map<string, number> = new Map();

  constructor(options: AlerterOptions = {}) {
    this.webhookUrl = options.webhookUrl ?? process.env.ALERT_WEBHOOK_URL ?? '';
    const rawRateLimit = options.rateLimitMs ?? Number(process.env.ALERT_RATE_LIMIT_MS ?? 60_000);
    this.rateLimitMs = Number.isFinite(rawRateLimit) && rawRateLimit >= 0 ? rawRateLimit : 60_000;
    this.fetchImpl =
      options.fetchImpl ??
      (typeof globalThis.fetch === 'function' ? globalThis.fetch : this.noopFetch);
  }

  get enabled(): boolean {
    return this.webhookUrl.trim().length > 0;
  }

  /**
   * Sends an alert if one has not been sent for `type` within the rate-limit
   * window. Returns `true` when the alert was delivered (or at least allowed
   * through the rate limiter), `false` when suppressed by rate limiting or when
   * no webhook is configured.
   */
  async notify(alert: Omit<AlertPayload, 'timestamp'>): Promise<boolean> {
    if (!this.enabled) {
      return false;
    }

    const now = Date.now();
    const lastSent = this.lastSentAt.get(alert.type) ?? 0;
    if (now - lastSent < this.rateLimitMs) {
      return false;
    }

    const payload: AlertPayload = { ...alert, timestamp: now };

    try {
      assertNoSecrets(payload);
    } catch (err) {
      console.error(err instanceof Error ? err.message : String(err));
      throw err;
    }

    this.lastSentAt.set(alert.type, now);

    try {
      const response = await this.fetchImpl(this.webhookUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      if (!response.ok) {
        console.error(`Alert delivery failed: HTTP ${response.status}`);
      }
    } catch (error) {
      console.error(
        `Alert delivery failed: ${error instanceof Error ? error.message : String(error)}`,
      );
    }

    return true;
  }

  /** Clears rate-limit state (mainly useful for tests). */
  reset(): void {
    this.lastSentAt.clear();
  }

  private async noopFetch(): Promise<Response> {
    return new Response(null, { status: 200 });
  }
}

function assertNoSecrets(value: unknown): void {
  if (value === null || value === undefined) {
    return;
  }
  if (typeof value === 'string') {
    if (/S[A-Z2-7]{55}/.test(value)) {
      throw new Error('Security assertion failed: Stellar secret key detected in alert payload');
    }
    if (/[0-9a-fA-F]{64}/.test(value)) {
      throw new Error('Security assertion failed: Hex secret key detected in alert payload');
    }
    if (/[A-Za-z0-9+/]{43}=/.test(value)) {
      throw new Error('Security assertion failed: Base64 secret key detected in alert payload');
    }
  } else if (typeof value === 'object') {
    const keys = Object.keys(value as Record<string, unknown>);
    for (const key of keys) {
      const lowerKey = key.toLowerCase();
      if (
        lowerKey.includes('secretkey') ||
        lowerKey.includes('privatekey') ||
        lowerKey === 'secret' ||
        lowerKey.includes('oracle_secret')
      ) {
        throw new Error(`Security assertion failed: Secret key field "${key}" detected in alert payload`);
      }
      assertNoSecrets((value as Record<string, unknown>)[key]);
    }
  }
}

