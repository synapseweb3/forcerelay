# ckb4ibc integration test

Tx generator: <https://github.com/ImJeremyHe/forcerelay-tests-tx/commit/8ef5c2ad167cef18a91ebd5351c1cbe186e76265>


Run tests:

```
CHAIN_COMMAND_PATHS=ckb ACCOUNT_PREFIXES=ckb RUST_LOG=info cargo test -p ckb4ibc-test
```
