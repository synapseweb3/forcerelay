# IBC test

This module extended the `ibc-test-framework` with `ckb4ibc` and `axon` chain types.

For easy to upgrade the upstream, we separate this crate from `ibc-test-framework`.

## IBC test framework notes

We use chain-A chain-B, connection-A connection-B or channel-A channel-B to represent the two chains's components. We can assume the chain A is the initiator in the IBC test framework's context.

1. The testing framework always opens an empty client/connection/channel on chain B side to force the chain B use `xxx-1` name instead of `xxx-0`. The reason for this behavior is to catch bugs or issues that might arise when using a different name rather than default name. This methdology can potentially uncover edge cases or problems related to naming conventions or conflicts.
2. The testing framework is designed for gaia chain, it assumes the chain A and chain B both has a builtin `transfer` module, and the module is registered to IBCHandler, in channel tests IBC framework opens channel to the `transfer` module if we do not override it.

## Known issues

`gaia` chain uses a builtin `transfer` port as the default port in IBC tests.

For Axon chain we use `transfer` as default port.

For CKB chain we uses `blake2b(b"transfer")` as default port.

The override is implemented in the `RunExtendedChannelTest`.

## Run tests

Environment variables:

``` bash
# IBC contracts
export IBC_CONTRACTS_SRC_PATH=<ibc solidity contract source path>
cd $IBC_CONTRACTS_SRC_PATH && yarn install && yarn compile

# Axon
export AXON_SRC_PATH=<axon source path>
cd $AXON_SRC_PATH
git checkout forcerelay-test
cargo install --path .

# Ckb 0.111 should be installed: https://github.com/nervosnetwork/ckb/releases/tag/v0.111.0
ckb --version

# Chains to test (chain_a,chain_b). You can also use `axon,ckb`, `ckb,ckb` or `axon,axon`
export CHAIN_COMMAND_PATHS=ckb,axon
export ACCOUNT_PREFIXES=ckb,axon
```


Run tests:

``` bash
# Ensure the envs are exported correctly before run tests!
RUST_LOG=info cargo test -p ibc-test
```
