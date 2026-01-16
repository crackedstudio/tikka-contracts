# Tikka - Decentralized Raffle Platform

![Tikka Logo](https://via.placeholder.com/200x100/4F46E5/FFFFFF?text=TIKKA)

## 🎯 What is Tikka?

Tikka is a decentralized raffle platform built on Stellar using Soroban smart contracts. Users can create raffles, sell tickets priced in Stellar assets, and distribute prizes securely on-chain.

## 🚀 Key Features

### **🎲 On-Chain Winner Selection (Demo)**

-   Deterministic winner selection derived from ledger data
-   Simple and transparent process for a demo contract
-   Designed for clarity, not production-grade randomness

### **💰 Token-Based Tickets and Prizes**

-   **Ticket Purchases**: Any Stellar asset contract
-   **Prizes**: Same asset used for ticket purchases
-   **Flexible Pricing**: Set ticket prices per raffle

### **🔒 Escrowed Prizes**

-   Prizes are held in the smart contract until finalization
-   Winners claim prizes after raffle completion

### **📊 Basic Raffle Analytics**

-   Total tickets sold per raffle
-   Winner tracking and claim status

## 🏗️ How Tikka Works

### **1. Raffle Creation**

```
Creator → Create Raffle → Set Parameters
```

-   Raffle creators specify:
    -   Description and end time
    -   Maximum ticket count
    -   Ticket price and payment asset
    -   Whether multiple tickets per person are allowed

### **2. Prize Escrow**

```
Creator → Deposit Prize → Contract Escrow
```

-   Prizes are transferred to the smart contract
-   Contract holds the prize until raffle finalization

### **3. Ticket Sales**

```
Participants → Buy Tickets → Contract Validation → Ticket Issuance
```

-   Users purchase tickets with the raffle asset
-   Contract validates payment and issues tickets

### **4. Winner Selection**

```
Raffle Ends → Finalize → Select Winner
```

-   Winner is selected from sold tickets
-   Selection uses ledger-derived data for demo purposes

### **5. Prize Distribution**

```
Winner Selected → Claim Prize
```

-   Winners claim their prizes

## 🔧 Technical Architecture

### **Smart Contract Stack**

-   **Soroban (Rust)**: Smart contract implementation
-   **Stellar**: Network and asset contracts

### **Core Contract**

#### **`contracts/hello-world/src/lib.rs`**

```rust
pub fn create_raffle(... ) -> u64;
pub fn buy_ticket(... ) -> u32;
pub fn finalize_raffle(... ) -> Address;
pub fn claim_prize(... );
```

### **Data Structures**

```rust
pub struct Raffle {
    pub id: u64,
    pub creator: Address,
    pub description: String,
    pub end_time: u64,
    pub max_tickets: u32,
    pub allow_multiple: bool,
    pub ticket_price: i128,
    pub payment_token: Address,
    pub prize_amount: i128,
    pub tickets_sold: u32,
    pub is_active: bool,
    pub prize_deposited: bool,
    pub prize_claimed: bool,
    pub winner: Option<Address>,
}
```

## 🌐 Deployed Contracts

### **Stellar Testnet**

-   **Contract Address**: _TBD_

## 🚀 Getting Started

### **Prerequisites**

-   Rust toolchain
-   Stellar CLI (optional for deployment)

### **Run Tests**

```bash
cargo test -p hello-world
```

### **Build the Contract**

```bash
cargo build -p hello-world
```

## 🛠️ Development

### **Local Development**

```bash
# Clone repository
git clone https://github.com/your-org/tikka-contracts.git
cd tikka-contracts

# Run tests
cargo test -p hello-world
```

## 📚 Documentation

-   **Stellar Soroban**: https://developers.stellar.org/docs/build/smart-contracts/overview
-   **Soroban Examples**: https://github.com/stellar/soroban-examples

## 🤝 Contributing

We welcome contributions! Please see our contributing guidelines and code of conduct.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🆘 Support

-   **Documentation**: Check our guides
-   **Issues**: Report bugs and feature requests
-   **Community**: Join our Discord for discussions

---

**Built with ❤️ on Stellar**

