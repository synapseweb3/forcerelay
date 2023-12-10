# Deployment of Forcerelay/Axon

dependencies and tools:
1. Axon Rpc (Alphanet): https://rpc-alphanet-axon.ckbapp.dev
2. CKB Rpc (Testnet): https://testnet.ckbapp.dev/
3. Yarn: v1.22.19
4. ckb-cli: v1.4.0
5. cargo: v1.74,0

related repos and commits:
1. ibc-solidity-contract: https://github.com/synapseweb3/ibc-solidity-contract
2. forcerelay-ckb-contracts: https://github.com/synapseweb3/forcerelay-ckb-contracts
3. forcerelay-ckb-sdk: https://github.com/synapseweb3/forcerelay-ckb-sdk

## Deploy `ibc-solidity-contracts`
deploy from the repo (Yarn needed):
```bash
$ git clone https://github.com/synapseweb3/ibc-solidity-contract
$ cd ibc-solidity-contract
$ yarn install
$ yarn compile
$ AXON_HTTP_RPC_URL=https://rpc-alphanet-axon.ckbapp.dev yarn migrate
```
 
after the deployment, contracts `OwnableIBCHandler` and `ICS20TransferERC20` are in your output, let's record the addresses of them as **<IBC_HANDLER_ADDRESS>** and **<TRANSFER_ERC20_ADDRESS>** for a later use.

## Deploy `forcerelay-ckb-contracts`

we have compiled all of the latest 4 ckb contracts, named `Connection`, `Channel`, `Packet` and `sudt-transfer`, in the repo of Forcerelay, please make sure there's enough CKB capacity in your ckb address **<your_ckb_address>** and follow the below steps (ckb-cli needed):
```bash
$ git clone https://github.com/synapseweb3/forcerelay
$ cd forcerelay/tools/ibc-test/contracts/deployment
$ ./upload.sh connection https://testnet.ckbapp.dev/ <your_ckb_address>
$ ./upload.sh channel https://testnet.ckbapp.dev/ <your_ckb_address>
$ ./upload.sh packet https://testnet.ckbapp.dev/ <your_ckb_address>
$ ./upload.sh sudt https://testnet.ckbapp.dev/ <your_ckb_address>
```

generated deployment files after that are located in the directory `./migration`, please find them and search transaction hashes in the CKB explorer, `args` of each type script in the first transaction `Output` can be found there.

let's record all of above `args`s as **<CONNECTION_TYPE_ARGS>**, **<CHANNEL_TYPE_ARGS>**, **<PACKET_TYPE_ARGS>**, and **<SUDT_TRANSFER_TYPE_ARGS>**, plus **<SUDT_TRANSFER_TYPE_ID>** which can be found right in the `sudt-transfer` migration file.

## Install Forcerelay/Axon
fetch CKB related info through Axon rpc and extract metadata cell deployment data, find its `code_hash` and `args` of type script, and record them as **<CLIENT_CODE_HASH>** and **<CLIENT_TYPE_ARGS>**.

to install Forcerelay/Axon:
```bash
$ git clone https://github.com/synapseweb3/forcerelay
$ cd forcerelay
$ cargo install --path crates/relayer-cli --bin forcerelay
```

create `config.toml` in the path `~/.forcerelay/`:
```toml
[global]
log_level = 'info'

[mode.clients]
enabled = true
refresh = true
misbehaviour = false

[mode.connections]
enabled = false

[mode.channels]
enabled = false

[mode.packets]
enabled = true
clear_interval = 100
clear_on_start = true
tx_confirmation = false
auto_register_counterparty_payee = false

[rest]
enabled = false
host = '127.0.0.1'
port = 3000

[telemetry]
enabled = false
host = '127.0.0.1'
port = 3001

[[chains]]
id = 'ckb4ibc-0'
ckb_rpc = 'https://testnet.ckbapp.dev/'
ckb_indexer_rpc = 'https://testnet.ckbapp.dev/'
key_name = 'relayer_ckb_wallet'
store_prefix = 'forcerelay'
client_code_hash = <CLIENT_COCE_HASH>
connection_type_args = <CONNECTION_TYPE_ARGS>
channel_type_args = <CHANNEL_TYPE_ARGS>
packet_type_args = <PACKET_TYPE_ARGS>

[chains.packet_filter]
policy = 'allowall'

[chains.packet_filter.min_fees]
[chains.onchain_light_clients.Axon]
chain_id = 'axon-0'
client_cell_type_args = <CLIENT_TYPE_ARGS>
ibc_handler_address = <IBC_HANDLER_ADDRESS>

[[chains]]
id = 'axon-0'
websocket_addr = "wss://rpc-alphanet-axon.ckbapp.dev/ws"
rpc_addr = "https://rpc-alphanet-axon.ckbapp.dev"
contract_address = <IBC_HANDLER_ADDRESS>
restore_block_count = 10000
key_name = "relayer_axon_wallet"
store_prefix = "forcerelay"
```

write your keys of Axon and CKB in two different files privately, named **<your_ckb_private_file>** and **<your_axon_private_file>**, and then, import them to the installed Forcerelay/Axon:
```bash
$ forcerelay keys add --chain ckb4ibc-0 --secret-file <your_ckb_privkey_file>
$ forcerelay keys add --chain axon-0 --secret-file <your_axon_privkey_file>
```

## Create IBC Connection
to create an initial IBC connection between Axon and CKB:
```bash
$ forcerelay create connection --a-chain axon-0 --b-chain ckb4ibc-0
```

the connetion id of Axon, which follows the pattern `"connection-{number}"`, can be found after the command, if this is an initial run, for example, it would be `connection-0`.

## Generate Port Id
port id in Axon for transfer use is always named `transfer`, but in CKB is different, it's the lock hash of deployed `sudt-transfer` cell, which `args` can be temporarily set as ZERO:
```toml
# lock script of sudt-transfer
code_hash = <SUDT_TRANSFER_TYPE_ID>
hash_type = type
args = 0x0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
```

calculate blake2b hash of above lock script, and let's record it as **<ST_CELL_LOCK_HASH>**.

## Create IBC Channel
to create IBC channel on the created IBC connection `connection-0`:
```bash
$ forcerelay create channel --a-chain axon-0 --a-connection connection-0 \
	--a-port transfer --b-port <ST_CELL_LOCK_HASH>
```

the channel id of Axon and CKB, which follows the pattern `"channel-{number}"`, can be found after the command, and for an initial run, it would be `channel-0` for both of Axon and CKB.

## Create `sudt-transfer` Cell
before the creation of `sudt-transfer` cell, to prepare the example binary of `forcerelay-ckb-sdk` is the prerequisites:
```bash
$ git clone https://github.com/synapseweb3/forcerelay-ckb-sdk
$ cd forcerelay-ckb-sdk
$ cargo install --path . --example sudt-transfer
$ sudt-transfer --help
```

create `sdk.config.toml` in the path `~/.forcerelay/`, which needs the information of our private sudt cell, let's record the `args` of its lock script as **<your_sudt_lock_args>** and the blake2b hash of its type script as **<your_sudt_type_hash>**:
```toml
channel_contract_type_id_args = <CHANNEL_TYPE_ARGS>
packet_contract_type_id_args = <PACKET_TYPE_ARGS>
channel_id = 0 # number 0 in channel id of “channel-0”
axon_ibc_handler_address = <IBC_HANDLER_ADDRESS>
confirmations = 3

private_key = <your_ckb_private_key>
ckb_rpc_url = "https://testnet.ckbapp.dev/"

[module_lock_script]
code_hash = <SUDT_TRANSFER_TYPE_ID>
hash_type = "type"
args = "0x0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"

[axon_metadata_type_script]
code_hash = <CLIENT_CODE_HASH>
hash_type = "type"
args = <CLIENT_TYPE_ARGS>

[sudt_transfer_contract_type_script]
code_hash = "0x00000000000000000000000000000000000000000000000000545950455f4944"
hash_type = "type"
args = <SUDT_TRANSFER_TYPE_ARGS>

[sudt.my_sudt]
code_hash = "0xc5e5dcf215925f7ef4dfaf5f4b4f105bc321c02776d6e7d52a1db3fcd9d011a4"
hash_type = "type"
args = <your_sudt_lock_args>
```

to create your `sudt-transfer` cell:
```bash
$ sudt-transfer --config ~/.forcerelay/sdk.config.toml create-st-cell --sudt my_sudt
```

## Send IBC Message from Axon to CKB
assume the receiver address in Axon is **<your_address_in_axon>**.

to trigger IBC message of `MsgSendPacket` in Axon:
```bash
$ git clone https://github.com/synapseweb3/ibc-solidity-contract
$ cd ibc-solidity-contract
$ RECEIVER=<your_address_in_axon> \ # start with 0x
	TRANSFER_CONTRACT_ADDRESS=<TRANSFER_ERC20_ADDRESS> \ # trim 0x
	CHANNEL=channel-0
	DENOM=<your_sudt_type_hash> \ # trim 0x
	AMOUNT=100 \ # amount of sudt token > 100
	yarn send
```

to receive `MsgSendPacket` and generate IBC message `MsgWritePacket`:
```bash
$ sudt-transfer --config ~/.forcerelay/sdk.config.toml recv
```
