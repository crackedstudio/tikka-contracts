import re

with open('contracts/raffle/src/lib.rs', 'r') as f:
    lib = f.read()
lib = lib.replace('PageResult_Raffles', 'PageResultRaffles')
with open('contracts/raffle/src/lib.rs', 'w') as f:
    f.write(lib)

with open('contracts/raffle/src/instance/test.rs', 'r') as f:
    test = f.read()

# Fix admin_client and token_client
test = test.replace('let token_client = token::Client::new(&env, &admin_client.address);', 'let token_client = token::Client::new(&env, &admin_client.address);')
# Actually, the error was admin_client not defined.
test = re.sub(
    r'let \(client, _, _, _, _, factory_admin\) = setup_raffle_env\((.*?)\);',
    r'let (client, _, _, admin_client, _, factory_admin) = setup_raffle_env(\1);',
    test,
    flags=re.DOTALL
)

# Replace remaining metadata_hash
test = re.sub(r'(tikka_token:\s*None,?\s*)\n(\s*})', r'\1\n        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),\n\2', test)

# Fix 3 args buy_tickets
test = test.replace('let total_sold = client.buy_tickets(&buyer, &3u32, &1);', 'let total_sold = client.buy_tickets(&buyer, &3u32);')
test = test.replace('let total_sold = client.buy_tickets(&buyer, &5u32, &1);', 'let total_sold = client.buy_tickets(&buyer, &5u32);')

# Fix obsolete fee methods
test = test.replace('let result = env.as_contract(&stranger, || client.try_set_fee_bps(&250));', '// let result = env.as_contract(&stranger, || client.try_set_fee_bps(&250));')
test = test.replace('client.set_fee_bps(&500);', '// client.set_fee_bps(&500);')
test = test.replace('client.set_treasury_address(&treasury);', '// client.set_treasury_address(&treasury);')
test = test.replace('client.set_fee_bps(&250);', '// client.set_fee_bps(&250);')

with open('contracts/raffle/src/instance/test.rs', 'w') as f:
    f.write(test)
