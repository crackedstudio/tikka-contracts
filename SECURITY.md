# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| main    | ✅        |
| older   | ❌        |

## Reporting a Vulnerability

**Do NOT open a public GitHub issue for security vulnerabilities.**

Report security issues by emailing: security@tikka.xyz (or via GitHub private vulnerability reporting)

Please include:
- Description of the vulnerability
- Affected contract(s) and function(s)
- Proof of concept (testnet transaction hash or test code)
- Estimated impact (funds at risk, scope of affected raffles)

### Response Timeline
- We will acknowledge receipt within 48 hours
- We will provide an initial assessment within 5 business days
- We aim to release a fix within 30 days for critical issues

## Known Trust Assumptions

- Internal PRNG (`RandomnessSource::Internal`) is NOT suitable for high-value raffles
- The oracle operator must be trusted in `RandomnessSource::External` mode
- Admin key compromise allows pausing and admin-cancelling raffles
