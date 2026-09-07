import { createLogger, childLogger } from './logger';

describe('logger', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv };
    delete process.env.LOG_LEVEL;
    delete process.env.NODE_ENV;
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  it('redacts secret key env var names from log messages', () => {
    const log = createLogger();
    const warnSpy = jest.spyOn(process.stdout, 'write').mockImplementation(() => true);

    log.warn('ORACLE_SECRET_KEY=SABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZ');

    expect(warnSpy).toHaveBeenCalledWith(
      expect.stringContaining('ORACLE_SECRET_KEY=[REDACTED]'),
    );
    warnSpy.mockRestore();
  });

  it('redacts hex/base64 key material from log messages', () => {
    const log = createLogger();
    const warnSpy = jest.spyOn(process.stdout, 'write').mockImplementation(() => true);

    log.warn('hex=ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZ');

    expect(warnSpy).toHaveBeenCalledWith(
      expect.stringContaining('[REDACTED]'),
    );
    warnSpy.mockRestore();
  });

  it('binds requestId and raffleId as child logger fields', () => {
    const child = childLogger({ requestId: '123', raffleId: 'CABC' });
    const writeSpy = jest.spyOn(process.stdout, 'write').mockImplementation(() => true);

    child.info('test');

    expect(writeSpy).toHaveBeenCalledWith(
      expect.stringContaining('"requestId":"123"'),
    );
    expect(writeSpy).toHaveBeenCalledWith(
      expect.stringContaining('"raffleId":"CABC"'),
    );
    writeSpy.mockRestore();
  });
});
