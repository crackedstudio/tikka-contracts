# Security Policy

## Supported Versions

We actively support and patch security issues in the following versions of the Raffle Oracle and Smart Contracts:

| Version | Supported |
| ------- | --------- |
| 1.x.x   | Yes       |
| < 1.0.0 | No        |

## Reporting a Vulnerability

If you discover a security vulnerability, please do **not** open a public issue. Instead, report it privately:

1. Send an email to **security@cracked.studio**.
2. Include a detailed description of the vulnerability, steps to reproduce it, and the potential impact.
3. We will acknowledge receipt of your vulnerability report within 48 hours and work with you to coordinate a security fix and release.

## Oracle Private Key Hardening

The oracle service handles sensitive private key material. To protect these credentials, we implement the following runtime security practices:

### 1. Zeroizable Buffer Key Handling
Private keys are never stored as plain strings in memory. They are loaded into zeroizable `Buffer` objects. Once the key has been processed or signed, the buffers are immediately filled with zeros to scrub the private key bytes from memory.

### 2. Environment Variable Cleansing
To prevent leakages (e.g., via diagnostic logs, child processes, or memory dumps), the `EnvSecretsAdapter` immediately deletes `ORACLE_SECRET_KEY` from `process.env` once parsed during bootstrap. In production mode, environment variables are blocked entirely, requiring the use of HashiCorp Vault.

### 3. Secure Key Vault
The service supports fetching keys dynamically over HTTPS from a secure Vault instance (`VaultSecretsAdapter`). Keys retrieved via Vault are similarly scrubbed from memory immediately after use.

### 4. Alert Payload Scanning
To prevent accidental leakage of secret keys or other sensitive materials through operational alerts (e.g., webhook integrations, Discord, Slack), all alert payloads are recursively scanned before dispatch. If any string or property matches the format of a private key or loaded secret, the dispatch is aborted and a secure placeholder warning is generated.
