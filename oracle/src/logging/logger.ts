import pino from 'pino';

const isProduction = process.env.NODE_ENV === 'production';
const logLevel = process.env.LOG_LEVEL ?? 'info';

const redactedFields = new Set([
  'ORACLE_SECRET_KEY',
  'secretKey',
  'secret',
  'password',
  'token',
  'apiKey',
  'api_key',
  'accessKey',
  'access_key',
  'privateKey',
  'private_key',
  'passphrase',
]);

function redactLogMessage(message: string): string {
  let redacted = message;
  for (const field of redactedFields) {
    const regex = new RegExp(`(${field}=)[^\\s"']+`, 'gi');
    redacted = redacted.replace(regex, '$1[REDACTED]');
  }
  const hexRegex = /(?:hex|base64|encoded)[:\s]+([A-Za-z0-9+/=]{32,})/gi;
  redacted = redacted.replace(hexRegex, '$1[REDACTED]');
  return redacted;
}

export interface LoggerOptions {
  requestId?: string;
  raffleId?: string;
}

export type Logger = pino.Logger;

export function createLogger(options: LoggerOptions = {}): Logger {
  const baseLogger = pino(
    {
      level: logLevel,
      formatter: isProduction
        ? undefined
        : {
            log(obj: Record<string, unknown>) {
              return `${obj.level}:${obj.msg}`;
            },
          },
      redact: ['secretKey', 'secret', 'password', 'token', 'apiKey', 'api_key', 'accessKey', 'access_key', 'privateKey', 'private_key', 'passphrase', 'req.headers.authorization', 'req.headers.cookie'],
    },
    isProduction
      ? undefined
      : pino.transport({
          target: 'pino-pretty',
          options: {
            colorize: true,
            translateTime: 'SYS:standard',
            ignore: 'pid,hostname',
          },
        })
  );

  const childBindings: Record<string, unknown> = {};
  if (options.requestId) {
    childBindings.requestId = options.requestId;
  }
  if (options.raffleId) {
    childBindings.raffleId = options.raffleId;
  }

  const logger = Object.keys(childBindings).length > 0 ? baseLogger.child(childBindings) : baseLogger;

  const originalInfo = logger.info.bind(logger);
  logger.info = (obj: unknown, ...args: unknown[]) => {
    if (typeof obj === 'string') {
      return originalInfo(redactLogMessage(obj), ...args);
    }
    return originalInfo(obj, ...args);
  };

  const originalWarn = logger.warn.bind(logger);
  logger.warn = (obj: unknown, ...args: unknown[]) => {
    if (typeof obj === 'string') {
      return originalWarn(redactLogMessage(obj), ...args);
    }
    return originalWarn(obj, ...args);
  };

  const originalError = logger.error.bind(logger);
  logger.error = (obj: unknown, ...args: unknown[]) => {
    if (typeof obj === 'string') {
      return originalError(redactLogMessage(obj), ...args);
    }
    return originalError(obj, ...args);
  };

  const originalDebug = logger.debug.bind(logger);
  logger.debug = (obj: unknown, ...args: unknown[]) => {
    if (typeof obj === 'string') {
      return originalDebug(redactLogMessage(obj), ...args);
    }
    return originalDebug(obj, ...args);
  };

  const originalTrace = logger.trace.bind(logger);
  logger.trace = (obj: unknown, ...args: unknown[]) => {
    if (typeof obj === 'string') {
      return originalTrace(redactLogMessage(obj), ...args);
    }
    return originalTrace(obj, ...args);
  };

  return logger;
}

export const logger = createLogger();

export function childLogger(options: LoggerOptions): Logger {
  return createLogger(options);
}
