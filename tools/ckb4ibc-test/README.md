# ckb4ibc integration test

Tx generator: <https://github.com/ImJeremyHe/forcerelay-tests-tx/commit/8ef5c2ad167cef18a91ebd5351c1cbe186e76265>

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
