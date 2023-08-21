# ckb4ibc integration test

Tx generator: <https://github.com/ImJeremyHe/forcerelay-tests-tx/commit/8ef5c2ad167cef18a91ebd5351c1cbe186e76265>

## IBC test framework notes

We use chain-A chain-B, connection-A connection-B or channel-A channel-B to represent the two chains's components. We can assume the chain A is the initiator in the IBC test framework's context.

1. The testing framework always opens an empty client/connection/channel on chain B side to force the chain B use `xxx-1` name instead of `xxx-0`, it is for catching bugs.
2. The testing framework is designed for gaia chain, it assumes the chain A and chain B both has a builtin `transfer` module, and the module is registered to IBCHandler, in channel tests IBC framework opens channel to the `transfer` module if we do not override it.

## Prepare

``` bash
# IBC contracts
export IBC_CONTRACTS_SRC_PATH = <ibc solidity contract source path>
cd $IBC_CONTRACTS_SRC_PATH && yarn install && yarn compile

# Axon
export AXON_SRC_PATH=<axon source path>
cd $AXON_SRC_PATH
git checkout forcerelay-test
cargo install --path .

# setup hermes envs
## CKB
export CHAIN_COMMAND_PATHS=ckb
export ACCOUNT_PREFIXES=ckb
## Axon
export CHAIN_COMMAND_PATHS=axon
export ACCOUNT_PREFIXES=axon
```


## Run tests

``` bash
# Ensure the envs are exported correctly before run tests!
RUST_LOG=info cargo test -p ckb4ibc-test
```
