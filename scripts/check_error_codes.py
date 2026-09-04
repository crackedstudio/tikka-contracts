#!/usr/bin/env python3
"""
CI check: ensure no duplicate or reused discriminants within each Rust error enum.

Usage:
    python scripts/check_error_codes.py

Exits non-zero if a duplicate discriminant is found within any single enum.
"""

import re
import sys
from pathlib import Path


def parse_error_enum(file_path, enum_name):
    with open(file_path, 'r') as f:
        content = f.read()
    
    enum_match = re.search(
        r'(?:#\[contracterror\].*?)?pub enum ' + enum_name + r' \{(.*?)\}',
        content,
        re.DOTALL
    )
    
    if not enum_match:
        return []
    
    enum_body = enum_match.group(1)
    error_pattern = r'(\w+)\s*=\s*(\d+)'
    errors = []
    
    for match in re.finditer(error_pattern, enum_body):
        name = match.group(1)
        code = int(match.group(2))
        errors.append((code, name))
    
    return errors


def check_duplicates(errors, enum_name):
    seen = {}
    for code, name in errors:
        if code in seen:
            print(f"ERROR: Duplicate discriminant {code} in {enum_name}: "
                  f"{seen[code]} and {name}")
            return False
        seen[code] = name
    return True


def main():
    repo_root = Path(__file__).parent.parent
    instance_file = repo_root / "contracts" / "raffle-instance" / "src" / "lib.rs"
    factory_file = repo_root / "contracts" / "raffle-factory" / "src" / "lib.rs"
    
    instance_errors = parse_error_enum(instance_file, "Error")
    factory_errors = parse_error_enum(factory_file, "ContractError")
    
    ok = True
    ok &= check_duplicates(instance_errors, "raffle-instance::Error")
    ok &= check_duplicates(factory_errors, "raffle-factory::ContractError")
    
    if not ok:
        print("\nCI check FAILED: duplicate discriminants found.")
        sys.exit(1)
    
    print("CI check PASSED: no duplicate discriminants.")
    sys.exit(0)


if __name__ == "__main__":
    main()
