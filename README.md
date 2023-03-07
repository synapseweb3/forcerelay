# Synapse Relayer

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg?logo=apache)](LICENSE)

Synapse Relayer aims to allow [CKB](https://github.com/nervosnetwork/ckb)
and chains built on [Axon](https://github.com/axonweb3/axon) gain the ability to
interact with Ethereum and Cosmos-SDK chains via [IBC protocol](https://github.com/cosmos/ibc).

Synapse Relayer is based on [Forcerelay](https://github.com/informalsystems/hermes), which helps Cosmos-SDK chains to interact with each other. Many thanks to them!

## **WARNING**

This repo is still in early stage. Your issues or PRs are welcome.

## Quick Start

Modify `~/.hermes/config.toml`. Here is an example:

```toml
[global]
log_level = 'info'

[mode]

[mode.clients]
enabled = true
refresh = true
misbehaviour = true

[mode.connections]
enabled = true

[mode.channels]
enabled = true

[mode.packets]
enabled = true
clear_interval = 100
clear_on_start = true
tx_confirmation = true

[telemetry]
enabled = true
host = '127.0.0.1'
port = 3001

[[chains]]
id = 'ibc-eth-0'
genesis_time = 1606824023
genesis_root = "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95"
initial_checkpoint = "0x428ce0b5f5bbed1fc2b3feb5d4152ae0fe98a80b1bfa8de36681868e81e9222a"
key_name = 'wallet'
rpc_addr_pool = [
  "https://www.lightclientdata.org",
  "https://beacon-nd-995-871-887.p2pify.com/c9dce41bab3e120f541e4ffb748efa60"
]
rpc_port = 8545
[chains.forks]
genesis = { epoch = 0, fork_version = "0x00000000" }
altair = { epoch = 74240, fork_version = "0x01000000" }
bellatrix = { epoch = 144896, fork_version = "0x02000000" }

[[chains]]
id = 'ibc-ckb-0'
ckb_rpc = "https://testnet.ckb.dev"
ckb_indexer_rpc = "https://testnet.ckb.dev/indexer"
lightclient_contract_typeargs = "0x81e682d4d6db6b6e552f5ae9db6fcba6dfc395930ff62d86f271a92e433f3a36"
key_name = "ckb_key_name"
```

Run command `hermes forcerelay --ethereum-chain-id ibc-eth-0 --ckb-chain-id ibc-ckb-0` to start up relay of ETH headers to CKB network.

Warn: relayer is still under rapid development and the configuration example and command above may be outdated.

## Rqeuirements

This project requires Rust `1.65.0`.
