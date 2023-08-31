# Forcerelay

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg?logo=apache)](LICENSE)

Forcerelay aims to allow [CKB](https://github.com/nervosnetwork/ckb)
and chains built on [Axon](https://github.com/axonweb3/axon) gain the ability to
interact with Ethereum and Cosmos-SDK chains via [IBC protocol](https://github.com/cosmos/ibc).

Forcerelay is based on [Hermes](https://github.com/informalsystems/hermes), which helps Cosmos-SDK chains to interact with each other. Many thanks to them!

## **WARNING**

This repo is still in early stage. Your issues or PRs are welcome.

## Using Forcerelay/Eth

Create or modify `~/.hermes/config.toml`. Here is an example:

```toml
[global]
log_level = 'trace'

[mode]

[mode.clients]
enabled = true
refresh = true
misbehaviour = true

[mode.connections]
enabled = false

[mode.channels]
enabled = false

[mode.packets]
enabled = true
clear_interval = 100
clear_on_start = true
tx_confirmation = true

[[chains]]
id = 'ibc-eth-1'
genesis_time = 1606824023
genesis_root = "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95"
initial_checkpoint = "0x6c668d888d2b892bf26c25ee937ed43b48048dedda971ff33ebaaae7b2bd3890"
key_name = 'relayer_eth_wallet'
rpc_addr_pool = [
  "https://www.lightclientdata.org",
]
rpc_port = 8545
[chains.forks]
genesis = { epoch = 0, fork_version = "0x00000000" }
altair = { epoch = 74240, fork_version = "0x01000000" }
bellatrix = { epoch = 144896, fork_version = "0x02000000" }
capella = { epoch = 194048, fork_version = "0x03000000" }

[[chains]]
id = 'ckb-multi-client-4-1'
ckb_rpc = "https://testnet.ckbapp.dev"
ckb_indexer_rpc = "https://testnet.ckbapp.dev"
lightclient_contract_typeargs = "0x75ca34f9f1c28cf16d160fc485d5ed4a2a8f34424ec5854a7579ca82e72b7671"
lightclient_lock_typeargs = "0x8d5300c03081b19a28a30d2e8202a467ab19fafa5285c86896db035e783186d5"
minimal_updates_count = 1
key_name = "relayer_ckb_wallet"
data_dir = "./ckb_mmr_storage_multi_client_4_1"
client_type_args = { cells_count = 4, type_id = "0x673e557da4c8381638fc808956aa27e384cf66d9a63899a6e4e932c2395f7a40" }
```

Run command `forcerelay eth-ckb --ethereum-chain-id ibc-eth-1 --ckb-chain-id ibc-ckb-1` to start up relay of ETH headers to CKB network.

Warn: relayer is still under rapid development and the configuration example and command above may be outdated.

## Using Forcerelay/Axon (WIP)

Run `cargo test -p ibc-test --lib --all-features -- tests::integration_test --exact --nocapture --ignored` to check Ckb endpoint, which tests connection and channel layers between two Ckb chains

Run `Forcerelay create connection --a-chain axon-0 --b-chain axon-1` and `Forcerelay create channel --a-chain axon-0 --a-connection connection-0 --a-port mock-port-0 --b-port mock-port-0` to create connection and channel between two Axon chains

## Rqeuirements

This project requires Rust `^1.65.0`.
