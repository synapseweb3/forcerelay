# Forcerelay

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg?logo=apache)](LICENSE)

Forcerelay aims to allow [CKB](https://github.com/nervosnetwork/ckb)
and chains built on [Axon](https://github.com/axonweb3/axon) gain the ability to
interact with Ethereum and Cosmos-SDK chains via [IBC protocol](https://github.com/cosmos/ibc).

Forcerelay is based on [Hermes](https://github.com/informalsystems/hermes), which helps Cosmos-SDK chains to interact with each other. Many thanks to them!

## **WARNING**

This repo is still in early stage. Your issues or PRs are welcome.

## Quick Start

Modify `~/.hermes/config.toml`. Here is an example:

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
id = 'ibc-ckb-1'
ckb_rpc = "https://testnet.ckbapp.dev"
ckb_indexer_rpc = "https://testnet.ckbapp.dev"
lightclient_contract_typeargs = "0xb7fcfa4ad253ddd60481bdd35331a634ff985fdb3b3fab4e1066cf97faf40315"
lightclient_lock_typeargs = "0xd6eb3b34c445a1fe914f4e22c13380cf3b6102d9c2b949a8544d1a2f72a8b6c7"
key_name = "relayer_ckb_wallet"
data_dir = "./ckb_mmr_storage"
```

Run command `forcerelay eth-ckb --ethereum-chain-id ibc-eth-1 --ckb-chain-id ibc-ckb-1` to start up relay of ETH headers to CKB network.

Warn: relayer is still under rapid development and the configuration example and command above may be outdated.

## Rqeuirements

This project requires Rust `^1.65.0`.
