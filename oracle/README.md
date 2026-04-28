# Oracle Service

This directory contains the off-chain oracle service for the Tikka Contracts. The oracle is responsible for generating randomness securely and submitting reveal transactions.

## Configuration & Key Security

The oracle requires a secure keypair to sign reveal transactions. The `KeyService` handles loading and securing this keypair at runtime.

### Local Development (Environment Variables)

For local development or testing, you can provide the secret key via environment variables. The `KeyService` uses the `EnvSecretsAdapter` by default.

1. Copy `.env.example` to `.env` (or set it in your environment):
   ```sh
   ORACLE_SECRET_KEY="S..."
   ```
2. The `KeyService` will automatically parse and validate this key on startup without logging or exposing it.

### Production Setup (Secrets Manager)

For production deployments, the `KeyService` supports fetching the key securely from a Secrets Manager (e.g., AWS Secrets Manager, HSM) rather than relying on environment variables.

1. Implement or use the `AwsSecretsAdapter` (or an equivalent adapter for your infrastructure) in `src/keys/key.service.ts`.
2. When initializing the application, instantiate `KeyService` with the production adapter:
   ```typescript
   import { KeyService, AwsSecretsAdapter } from './keys/key.service';

   // Example: Using AWS Secrets Manager in Production
   const adapter = new AwsSecretsAdapter('us-east-1');
   const keyService = new KeyService(adapter, 'prod/oracle/secret_key');
   
   await keyService.initialize(); // Validates the key securely
   ```

## Architecture

* **`KeyService` (`src/keys/key.service.ts`)**: securely loads the keypair and exposes `.getKeypair()`, `.getPublicKey()`, and `.sign()`.
* **`VrfService` (`src/vrf/vrf.service.ts`)**: consumes `KeyService` for identifying and signing randomness proofs.
* **`TxSubmitterService` (`src/tx/tx-submitter.service.ts`)**: consumes `KeyService` for signing Stellar transactions before network submission.
