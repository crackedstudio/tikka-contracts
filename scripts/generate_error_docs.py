#!/usr/bin/env python3
"""
Script to auto-generate error code documentation from Rust Error enum.
This script parses the Error enum in contracts/raffle-instance/src/lib.rs
and generates the markdown table for docs/ERRORS.md.
"""

import re
import sys
from pathlib import Path


def parse_error_enum(file_path):
    """Parse the Error enum from the Rust source file."""
    with open(file_path, 'r') as f:
        content = f.read()
    
    # Find the Error enum
    enum_match = re.search(
        r'#\[contracterror\].*?pub enum Error \{(.*?)\}',
        content,
        re.DOTALL
    )
    
    if not enum_match:
        print("Error: Could not find Error enum in the file")
        sys.exit(1)
    
    enum_body = enum_match.group(1)
    
    # Parse each variant with its discriminant
    error_pattern = r'(\w+)\s*=\s*(\d+)'
    errors = []
    
    for match in re.finditer(error_pattern, enum_body):
        name = match.group(1)
        code = int(match.group(2))
        errors.append((code, name))
    
    # Sort by code
    errors.sort(key=lambda x: x[0])
    
    return errors


def generate_markdown_table(errors):
    """Generate markdown table from error list."""
    lines = []
    lines.append("| Code | Error | Description | Frontend Message |")
    lines.append("| ---- | ----- | ----------- | ---------------- |")
    
    # Define descriptions and messages based on error names
    descriptions = {
        'RaffleNotFound': 'The raffle data was not found in storage',
        'RaffleInactive': 'The raffle is not in an active state',
        'TicketsSoldOut': 'All tickets have been sold',
        'InsufficientFunds': 'User does not have enough balance',
        'NotAuthorized': 'User is not authorized to perform this action',
        'OracleNotSet': 'Oracle address is not configured',
        'RandomnessAlreadyRequested': 'Randomness has already been requested',
        'NoRandomnessRequest': 'No randomness request found',
        'FallbackTooEarly': 'Fallback randomness triggered too early',
        'PrizeNotDeposited': 'Prize has not been deposited yet',
        'PrizeAlreadyClaimed': 'Prize has already been claimed',
        'PrizeAlreadyDeposited': 'Prize deposit was already completed',
        'NotWinner': 'Only the winner can claim the prize',
        'ClaimTooEarly': 'Cannot claim before cooldown period',
        'InvalidParameters': 'Invalid input parameters provided',
        'InvalidQuantity': 'Invalid ticket quantity requested',
        'InvalidStatus': 'The current raffle status doesn\'t allow this operation',
        'ContractPaused': 'The contract is paused',
        'InvalidStateTransition': 'Cannot transition to the requested state',
        'RaffleExpired': 'The raffle end time has passed',
        'InsufficientTickets': 'Not enough tickets sold to finalize',
        'MultipleTicketsNotAllowed': 'User already has a ticket',
        'NoTicketsSold': 'No tickets have been purchased',
        'TicketNotFound': 'The requested ticket was not found',
        'RaffleEnded': 'The raffle has already ended',
        'ArithmeticOverflow': 'Arithmetic operation overflow',
        'AlreadyInitialized': 'Contract is already initialized',
        'NotInitialized': 'Contract has not been initialized',
        'Reentrancy': 'Reentrant call detected',
        'TokenTransferFailed': 'Token transfer failed',
        'NoActiveTickets': 'No active tickets available',
        'DeadlinePassed': 'Swap deadline has passed',
        'SlippageExceeded': 'Slippage tolerance exceeded',
        'InvalidIndex': 'Invalid index provided',
        'MorePrizesThanTickets': 'More prizes than tickets',
        'ZeroPrize': 'Prize amount is zero',
        'InvalidTokenAddress': 'Invalid token address provided',
        'TooManyPrizes': 'Exceeds maximum number of prizes',
        'EmergencyTooEarly': 'Emergency withdraw too early',
        'InvalidTicketRange': 'Invalid ticket range configured',
        'InsufficientAccumulatedFees': 'Insufficient accumulated fees',
        'PrizeConfigurationLocked': 'Prize configuration is locked',
        'ExceedsMaxTicketsPerTx': 'Exceeds max tickets per transaction',
    }
    
    messages = {
        'RaffleNotFound': '"Raffle not found"',
        'RaffleInactive': '"This raffle is not currently active"',
        'TicketsSoldOut': '"Sorry, all tickets have been sold!"',
        'InsufficientFunds': '"Insufficient funds to complete this action"',
        'NotAuthorized': '"You are not authorized to perform this action"',
        'OracleNotSet': '"Oracle address is not set"',
        'RandomnessAlreadyRequested': '"Randomness request already in progress"',
        'NoRandomnessRequest': '"No randomness request found"',
        'FallbackTooEarly': '"Fallback randomness not available yet"',
        'PrizeNotDeposited': '"Prize not yet deposited"',
        'PrizeAlreadyClaimed': '"Prize has already been claimed"',
        'PrizeAlreadyDeposited': '"Prize has already been deposited"',
        'NotWinner': '"You are not the winner of this raffle"',
        'ClaimTooEarly': '"Please wait before claiming your prize"',
        'InvalidParameters': '"Invalid parameters provided"',
        'InvalidQuantity': '"Invalid ticket quantity"',
        'InvalidStatus': '"This action is not allowed in the current raffle state"',
        'ContractPaused': '"Contract is temporarily paused"',
        'InvalidStateTransition': '"Cannot change raffle to the requested state"',
        'RaffleExpired': '"This raffle has ended"',
        'InsufficientTickets': '"Minimum ticket requirement not met"',
        'MultipleTicketsNotAllowed': '"Multiple tickets not allowed for this raffle"',
        'NoTicketsSold': '"No tickets have been sold yet"',
        'TicketNotFound': '"Ticket not found"',
        'RaffleEnded': '"This raffle has already ended"',
        'ArithmeticOverflow': '"Calculation error occurred"',
        'AlreadyInitialized': '"Contract already initialized"',
        'NotInitialized': '"Contract not initialized"',
        'Reentrancy': '"Please try again later"',
        'TokenTransferFailed': '"Token transfer failed"',
        'NoActiveTickets': '"No active tickets available"',
        'DeadlinePassed': '"Swap deadline has passed"',
        'SlippageExceeded': '"Slippage tolerance exceeded"',
        'InvalidIndex': '"Invalid index provided"',
        'MorePrizesThanTickets': '"More prizes than tickets"',
        'ZeroPrize': '"Prize amount cannot be zero"',
        'InvalidTokenAddress': '"Invalid token address"',
        'TooManyPrizes': '"Too many prizes configured"',
        'EmergencyTooEarly': '"Emergency withdraw not available yet"',
        'InvalidTicketRange': '"Invalid ticket range"',
        'InsufficientAccumulatedFees': '"Insufficient accumulated fees"',
        'PrizeConfigurationLocked': '"Prize configuration is locked"',
        'ExceedsMaxTicketsPerTx': '"Too many tickets for one transaction"',
    }
    
    for code, name in errors:
        desc = descriptions.get(name, 'TODO: Add description')
        msg = messages.get(name, 'TODO: Add message')
        lines.append(f"| {code} | `{name}` | {desc} | {msg} |")
    
    return '\n'.join(lines)


def generate_typescript_mapping(errors):
    """Generate TypeScript error mapping from error list."""
    lines = []
    lines.append("const errorMessages: Record<number, string> = {")
    lines.append("  // Instance errors (1-58)")
    
    messages = {
        'RaffleNotFound': 'Raffle not found',
        'RaffleInactive': 'This raffle is not currently active',
        'TicketsSoldOut': 'Sorry, all tickets have been sold!',
        'InsufficientFunds': 'Insufficient funds to complete this action',
        'NotAuthorized': 'You are not authorized to perform this action',
        'OracleNotSet': 'Oracle address is not set',
        'RandomnessAlreadyRequested': 'Randomness request already in progress',
        'NoRandomnessRequest': 'No randomness request found',
        'FallbackTooEarly': 'Fallback randomness not available yet',
        'PrizeNotDeposited': 'Prize not yet deposited',
        'PrizeAlreadyClaimed': 'Prize has already been claimed',
        'PrizeAlreadyDeposited': 'Prize has already been deposited',
        'NotWinner': 'You are not the winner of this raffle',
        'ClaimTooEarly': 'Please wait before claiming your prize',
        'InvalidParameters': 'Invalid parameters provided',
        'InvalidQuantity': 'Invalid ticket quantity',
        'InvalidStatus': 'This action is not allowed in the current raffle state',
        'ContractPaused': 'Contract is temporarily paused',
        'InvalidStateTransition': 'Cannot change raffle to the requested state',
        'RaffleExpired': 'This raffle has ended',
        'InsufficientTickets': 'Minimum ticket requirement not met',
        'MultipleTicketsNotAllowed': 'Multiple tickets not allowed for this raffle',
        'NoTicketsSold': 'No tickets have been sold yet',
        'TicketNotFound': 'Ticket not found',
        'RaffleEnded': 'This raffle has already ended',
        'ArithmeticOverflow': 'Calculation error occurred',
        'AlreadyInitialized': 'Contract already initialized',
        'NotInitialized': 'Contract not initialized',
        'Reentrancy': 'Please try again later',
        'TokenTransferFailed': 'Token transfer failed',
        'NoActiveTickets': 'No active tickets available',
        'DeadlinePassed': 'Swap deadline has passed',
        'SlippageExceeded': 'Slippage tolerance exceeded',
        'InvalidIndex': 'Invalid index provided',
        'MorePrizesThanTickets': 'More prizes than tickets',
        'ZeroPrize': 'Prize amount cannot be zero',
        'InvalidTokenAddress': 'Invalid token address',
        'TooManyPrizes': 'Too many prizes configured',
        'EmergencyTooEarly': 'Emergency withdraw not available yet',
        'InvalidTicketRange': 'Invalid ticket range',
        'InsufficientAccumulatedFees': 'Insufficient accumulated fees',
        'PrizeConfigurationLocked': 'Prize configuration is locked',
        'ExceedsMaxTicketsPerTx': 'Too many tickets for one transaction',
    }
    
    for code, name in errors:
        msg = messages.get(name, 'TODO: Add message')
        lines.append(f"  {code}: \"{msg}\",")
    
    lines.append("};")
    return '\n'.join(lines)


def main():
    # Get the repository root
    repo_root = Path(__file__).parent.parent
    rust_file = repo_root / "contracts" / "raffle-instance" / "src" / "lib.rs"
    
    if not rust_file.exists():
        print(f"Error: Rust file not found at {rust_file}")
        sys.exit(1)
    
    # Parse the error enum
    errors = parse_error_enum(rust_file)
    
    print(f"Found {len(errors)} error codes")
    print("\nMarkdown Table:")
    print(generate_markdown_table(errors))
    print("\nTypeScript Mapping:")
    print(generate_typescript_mapping(errors))


if __name__ == "__main__":
    main()
