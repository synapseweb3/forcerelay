pub use ibc::*;
#[allow(clippy::too_many_arguments, non_camel_case_types)]
pub mod ibc {
    #![allow(clippy::enum_variant_names)]
    #![allow(dead_code)]
    #![allow(clippy::type_complexity)]
    #![allow(unused_imports)]
    #![allow(unused_qualifications)]
    use ethers::contract::{
        builders::{ContractCall, Event},
        Contract, Lazy,
    };
    use ethers::core::{
        abi::{Abi, Detokenize, InvalidOutputType, Token, Tokenizable},
        types::*,
    };
    use ethers::providers::Middleware;
    #[doc = "IBC was auto-generated with ethers-rs Abigen. More information at: https://github.com/gakonst/ethers-rs"]
    use std::sync::Arc;
    # [rustfmt :: skip] const __ABI : & str = "[{\"inputs\":[{\"internalType\":\"string\",\"name\":\"client_id\",\"type\":\"string\",\"components\":[],\"indexed\":false},{\"internalType\":\"uint256\",\"name\":\"client_type\",\"type\":\"uint256\",\"components\":[],\"indexed\":true},{\"internalType\":\"uint256\",\"name\":\"number\",\"type\":\"uint256\",\"components\":[],\"indexed\":false}],\"type\":\"event\",\"name\":\"CreateClient\",\"outputs\":[],\"anonymous\":false},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"role\",\"type\":\"bytes32\",\"components\":[],\"indexed\":true},{\"internalType\":\"bytes32\",\"name\":\"previousAdminRole\",\"type\":\"bytes32\",\"components\":[],\"indexed\":true},{\"internalType\":\"bytes32\",\"name\":\"newAdminRole\",\"type\":\"bytes32\",\"components\":[],\"indexed\":true}],\"type\":\"event\",\"name\":\"RoleAdminChanged\",\"outputs\":[],\"anonymous\":false},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"role\",\"type\":\"bytes32\",\"components\":[],\"indexed\":true},{\"internalType\":\"address\",\"name\":\"account\",\"type\":\"address\",\"components\":[],\"indexed\":true},{\"internalType\":\"address\",\"name\":\"sender\",\"type\":\"address\",\"components\":[],\"indexed\":true}],\"type\":\"event\",\"name\":\"RoleGranted\",\"outputs\":[],\"anonymous\":false},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"role\",\"type\":\"bytes32\",\"components\":[],\"indexed\":true},{\"internalType\":\"address\",\"name\":\"account\",\"type\":\"address\",\"components\":[],\"indexed\":true},{\"internalType\":\"address\",\"name\":\"sender\",\"type\":\"address\",\"components\":[],\"indexed\":true}],\"type\":\"event\",\"name\":\"RoleRevoked\",\"outputs\":[],\"anonymous\":false},{\"inputs\":[{\"internalType\":\"string\",\"name\":\"client_id\",\"type\":\"string\",\"components\":[],\"indexed\":false},{\"internalType\":\"uint256\",\"name\":\"client_type\",\"type\":\"uint256\",\"components\":[],\"indexed\":true},{\"internalType\":\"uint256\",\"name\":\"number\",\"type\":\"uint256\",\"components\":[],\"indexed\":false}],\"type\":\"event\",\"name\":\"UpdateClient\",\"outputs\":[],\"anonymous\":false},{\"inputs\":[],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"DEFAULT_ADMIN_ROLE\",\"outputs\":[{\"internalType\":\"bytes32\",\"name\":\"\",\"type\":\"bytes32\",\"components\":[]}]},{\"inputs\":[],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"IBC_RELAYER\",\"outputs\":[{\"internalType\":\"bytes32\",\"name\":\"\",\"type\":\"bytes32\",\"components\":[]}]},{\"inputs\":[{\"internalType\":\"struct MsgAckPacket\",\"name\":\"ackPacket\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"struct Packet\",\"name\":\"packet\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"sequence\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"source_port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"source_channel_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"destination_port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"destination_channel_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"payload\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes32\",\"name\":\"timeout_height\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"timeout_timestamp\",\"type\":\"uint256\",\"components\":[]}]},{\"internalType\":\"bytes\",\"name\":\"acknowledgement\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"struct Proofs\",\"name\":\"proofs\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"height\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"object_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"client_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"consensus_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"other_proof\",\"type\":\"bytes\",\"components\":[]}]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"ack_packet\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgChannelCloseConfirm\",\"name\":\"closeConfirm\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"struct ChannelId\",\"name\":\"channel_id\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"channel_id\",\"type\":\"string\",\"components\":[]}]},{\"internalType\":\"struct Proofs\",\"name\":\"proofs\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"height\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"object_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"client_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"consensus_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"other_proof\",\"type\":\"bytes\",\"components\":[]}]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"channel_close_confirm\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgChannelCloseInit\",\"name\":\"closeInit\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"struct ChannelId\",\"name\":\"channel_id\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"channel_id\",\"type\":\"string\",\"components\":[]}]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"channel_close_init\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgChannelOpenAck\",\"name\":\"openAck\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"struct ChannelId\",\"name\":\"channel_id\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"channel_id\",\"type\":\"string\",\"components\":[]}]},{\"internalType\":\"struct ChannelId\",\"name\":\"counterparty_channel_id\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"channel_id\",\"type\":\"string\",\"components\":[]}]},{\"internalType\":\"string\",\"name\":\"counterparty_version\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"struct Proofs\",\"name\":\"proofs\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"height\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"object_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"client_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"consensus_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"other_proof\",\"type\":\"bytes\",\"components\":[]}]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"channel_open_ack\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgChannelOpenConfirm\",\"name\":\"openConfirm\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"struct ChannelId\",\"name\":\"channel_id\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"channel_id\",\"type\":\"string\",\"components\":[]}]},{\"internalType\":\"struct Proofs\",\"name\":\"proofs\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"height\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"object_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"client_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"consensus_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"other_proof\",\"type\":\"bytes\",\"components\":[]}]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"channel_open_confirm\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgChannelOpenInit\",\"name\":\"openInit\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"struct ChannelEnd\",\"name\":\"channel\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"struct ChannelId\",\"name\":\"channel_id\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"channel_id\",\"type\":\"string\",\"components\":[]}]},{\"internalType\":\"enum STATE\",\"name\":\"state\",\"type\":\"uint8\",\"components\":[]},{\"internalType\":\"enum ORDERING\",\"name\":\"odering\",\"type\":\"uint8\",\"components\":[]},{\"internalType\":\"struct ChannelId\",\"name\":\"remote\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"channel_id\",\"type\":\"string\",\"components\":[]}]},{\"internalType\":\"string[]\",\"name\":\"connection_hops\",\"type\":\"string[]\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"version\",\"type\":\"string\",\"components\":[]}]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"channel_open_init\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgChannelOpenTry\",\"name\":\"openTry\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"struct ChannelId\",\"name\":\"previous_channel_id\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"channel_id\",\"type\":\"string\",\"components\":[]}]},{\"internalType\":\"struct ChannelEnd\",\"name\":\"channel\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"struct ChannelId\",\"name\":\"channel_id\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"channel_id\",\"type\":\"string\",\"components\":[]}]},{\"internalType\":\"enum STATE\",\"name\":\"state\",\"type\":\"uint8\",\"components\":[]},{\"internalType\":\"enum ORDERING\",\"name\":\"odering\",\"type\":\"uint8\",\"components\":[]},{\"internalType\":\"struct ChannelId\",\"name\":\"remote\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"channel_id\",\"type\":\"string\",\"components\":[]}]},{\"internalType\":\"string[]\",\"name\":\"connection_hops\",\"type\":\"string[]\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"version\",\"type\":\"string\",\"components\":[]}]},{\"internalType\":\"string\",\"name\":\"counterparty_version\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"struct Proofs\",\"name\":\"proofs\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"height\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"object_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"client_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"consensus_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"other_proof\",\"type\":\"bytes\",\"components\":[]}]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"channel_open_try\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgClientCreate\",\"name\":\"create\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"struct ClientState\",\"name\":\"client\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"chain_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"enum CLIENT_TYPE\",\"name\":\"client_type\",\"type\":\"uint8\",\"components\":[]},{\"internalType\":\"bytes32\",\"name\":\"latest_height\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"bytes32\",\"name\":\"frozen_height\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"trusting_period\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"max_clock_drift\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"extra_payload\",\"type\":\"bytes\",\"components\":[]}]},{\"internalType\":\"struct ConsensusState\",\"name\":\"consensus\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"timestamp\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes32\",\"name\":\"commitment_root\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"extra_payload\",\"type\":\"bytes\",\"components\":[]}]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"client_create\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgClientMisbehaviour\",\"name\":\"misbehaviour\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"client_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"header1_bytes\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"header2_bytes\",\"type\":\"bytes\",\"components\":[]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"client_misbehaviour\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgClientUpdate\",\"name\":\"update\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"client_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"header_bytes\",\"type\":\"bytes\",\"components\":[]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"client_update\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgTimeoutPacket\",\"name\":\"closePacket\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"struct Packet\",\"name\":\"packet\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"sequence\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"source_port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"source_channel_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"destination_port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"destination_channel_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"payload\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes32\",\"name\":\"timeout_height\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"timeout_timestamp\",\"type\":\"uint256\",\"components\":[]}]},{\"internalType\":\"uint256\",\"name\":\"next_sequence_recv\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"struct Proofs\",\"name\":\"proofs\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"height\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"object_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"client_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"consensus_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"other_proof\",\"type\":\"bytes\",\"components\":[]}]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"close_packet\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgConnectionOpenAck\",\"name\":\"openAck\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"connection_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"counterparty_connection_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"struct ClientState\",\"name\":\"client_state\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"chain_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"enum CLIENT_TYPE\",\"name\":\"client_type\",\"type\":\"uint8\",\"components\":[]},{\"internalType\":\"bytes32\",\"name\":\"latest_height\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"bytes32\",\"name\":\"frozen_height\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"trusting_period\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"max_clock_drift\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"extra_payload\",\"type\":\"bytes\",\"components\":[]}]},{\"internalType\":\"struct Proofs\",\"name\":\"proofs\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"height\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"object_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"client_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"consensus_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"other_proof\",\"type\":\"bytes\",\"components\":[]}]},{\"internalType\":\"string\",\"name\":\"version\",\"type\":\"string\",\"components\":[]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"connection_open_ack\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgConnectionOpenConfirm\",\"name\":\"openConfirm\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"connection_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"struct Proofs\",\"name\":\"proofs\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"height\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"object_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"client_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"consensus_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"other_proof\",\"type\":\"bytes\",\"components\":[]}]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"connection_open_confirm\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgConnectionOpenInit\",\"name\":\"openInit\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"client_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"struct ConnectionId\",\"name\":\"counterparty\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"client_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"connection_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"commitment_prefix\",\"type\":\"bytes\",\"components\":[]}]},{\"internalType\":\"string\",\"name\":\"version\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"delay_duration\",\"type\":\"uint256\",\"components\":[]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"connection_open_init\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"struct MsgConnectionOpenTry\",\"name\":\"openTry\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"previous_connection_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"client_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"struct ClientState\",\"name\":\"client_state\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"chain_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"enum CLIENT_TYPE\",\"name\":\"client_type\",\"type\":\"uint8\",\"components\":[]},{\"internalType\":\"bytes32\",\"name\":\"latest_height\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"bytes32\",\"name\":\"frozen_height\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"trusting_period\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"max_clock_drift\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"extra_payload\",\"type\":\"bytes\",\"components\":[]}]},{\"internalType\":\"struct ConnectionId\",\"name\":\"counterparty\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"string\",\"name\":\"client_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"connection_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"commitment_prefix\",\"type\":\"bytes\",\"components\":[]}]},{\"internalType\":\"string[]\",\"name\":\"counterparty_versions\",\"type\":\"string[]\",\"components\":[]},{\"internalType\":\"struct Proofs\",\"name\":\"proofs\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"height\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"object_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"client_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"consensus_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"other_proof\",\"type\":\"bytes\",\"components\":[]}]},{\"internalType\":\"uint256\",\"name\":\"delay_period\",\"type\":\"uint256\",\"components\":[]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"connection_open_try\",\"outputs\":[]},{\"inputs\":[],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"construct\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"role\",\"type\":\"bytes32\",\"components\":[]}],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"getRoleAdmin\",\"outputs\":[{\"internalType\":\"bytes32\",\"name\":\"\",\"type\":\"bytes32\",\"components\":[]}]},{\"inputs\":[{\"internalType\":\"string\",\"name\":\"client_id\",\"type\":\"string\",\"components\":[]}],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"get_light_client\",\"outputs\":[{\"internalType\":\"address\",\"name\":\"\",\"type\":\"address\",\"components\":[]}]},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"role\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"address\",\"name\":\"account\",\"type\":\"address\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"grantRole\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"role\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"address\",\"name\":\"account\",\"type\":\"address\",\"components\":[]}],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"hasRole\",\"outputs\":[{\"internalType\":\"bool\",\"name\":\"\",\"type\":\"bool\",\"components\":[]}]},{\"inputs\":[{\"internalType\":\"struct MsgRecvPacket\",\"name\":\"recvPacket\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"struct Packet\",\"name\":\"packet\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"sequence\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"source_port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"source_channel_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"destination_port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"destination_channel_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"payload\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes32\",\"name\":\"timeout_height\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"timeout_timestamp\",\"type\":\"uint256\",\"components\":[]}]},{\"internalType\":\"struct Proofs\",\"name\":\"proofs\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"height\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"object_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"client_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"consensus_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"other_proof\",\"type\":\"bytes\",\"components\":[]}]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"recv_packet\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"role\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"address\",\"name\":\"account\",\"type\":\"address\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"renounceRole\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"role\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"address\",\"name\":\"account\",\"type\":\"address\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"revokeRole\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"enum CLIENT_TYPE\",\"name\":\"client_type\",\"type\":\"uint8\",\"components\":[]},{\"internalType\":\"address\",\"name\":\"light_client\",\"type\":\"address\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"set_light_client\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"bool\",\"name\":\"pause\",\"type\":\"bool\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"set_pause\",\"outputs\":[]},{\"inputs\":[{\"internalType\":\"bytes4\",\"name\":\"interfaceId\",\"type\":\"bytes4\",\"components\":[]}],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"supportsInterface\",\"outputs\":[{\"internalType\":\"bool\",\"name\":\"\",\"type\":\"bool\",\"components\":[]}]},{\"inputs\":[{\"internalType\":\"struct MsgTimeoutPacket\",\"name\":\"timeoutPacket\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"struct Packet\",\"name\":\"packet\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"sequence\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"source_port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"source_channel_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"destination_port_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"string\",\"name\":\"destination_channel_id\",\"type\":\"string\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"payload\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes32\",\"name\":\"timeout_height\",\"type\":\"bytes32\",\"components\":[]},{\"internalType\":\"uint256\",\"name\":\"timeout_timestamp\",\"type\":\"uint256\",\"components\":[]}]},{\"internalType\":\"uint256\",\"name\":\"next_sequence_recv\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"struct Proofs\",\"name\":\"proofs\",\"type\":\"tuple\",\"components\":[{\"internalType\":\"uint256\",\"name\":\"height\",\"type\":\"uint256\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"object_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"client_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"consensus_proof\",\"type\":\"bytes\",\"components\":[]},{\"internalType\":\"bytes\",\"name\":\"other_proof\",\"type\":\"bytes\",\"components\":[]}]}]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"timeout_packet\",\"outputs\":[]}]" ;
    #[doc = r" The parsed JSON-ABI of the contract."]
    pub static IBC_ABI: ethers::contract::Lazy<ethers::core::abi::Abi> =
        ethers::contract::Lazy::new(|| {
            ethers::core::utils::__serde_json::from_str(__ABI).expect("invalid abi")
        });
    pub struct IBC<M>(ethers::contract::Contract<M>);
    impl<M> Clone for IBC<M> {
        fn clone(&self) -> Self {
            IBC(self.0.clone())
        }
    }
    impl<M> std::ops::Deref for IBC<M> {
        type Target = ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> std::fmt::Debug for IBC<M> {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.debug_tuple(stringify!(IBC))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ethers::providers::Middleware> IBC<M> {
        #[doc = r" Creates a new contract instance with the specified `ethers`"]
        #[doc = r" client at the given `Address`. The contract derefs to a `ethers::Contract`"]
        #[doc = r" object"]
        pub fn new<T: Into<ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            ethers::contract::Contract::new(address.into(), IBC_ABI.clone(), client).into()
        }
        #[doc = "Calls the contract's `DEFAULT_ADMIN_ROLE` (0xa217fddf) function"]
        pub fn default_admin_role(&self) -> ethers::contract::builders::ContractCall<M, [u8; 32]> {
            self.0
                .method_hash([162, 23, 253, 223], ())
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `IBC_RELAYER` (0xc02d5e55) function"]
        pub fn ibc_relayer(&self) -> ethers::contract::builders::ContractCall<M, [u8; 32]> {
            self.0
                .method_hash([192, 45, 94, 85], ())
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `ack_packet` (0xaa2d8efd) function"]
        pub fn ack_packet(
            &self,
            ack_packet: MsgAckPacket,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([170, 45, 142, 253], (ack_packet,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `channel_close_confirm` (0x22744a5f) function"]
        pub fn channel_close_confirm(
            &self,
            close_confirm: MsgChannelCloseConfirm,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([34, 116, 74, 95], (close_confirm,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `channel_close_init` (0x8cba9245) function"]
        pub fn channel_close_init(
            &self,
            close_init: MsgChannelCloseInit,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([140, 186, 146, 69], (close_init,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `channel_open_ack` (0xeaddeb7c) function"]
        pub fn channel_open_ack(
            &self,
            open_ack: MsgChannelOpenAck,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([234, 221, 235, 124], (open_ack,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `channel_open_confirm` (0x2d2ab8b4) function"]
        pub fn channel_open_confirm(
            &self,
            open_confirm: MsgChannelOpenConfirm,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([45, 42, 184, 180], (open_confirm,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `channel_open_init` (0xc1e2854a) function"]
        pub fn channel_open_init(
            &self,
            open_init: MsgChannelOpenInit,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([193, 226, 133, 74], (open_init,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `channel_open_try` (0x329df391) function"]
        pub fn channel_open_try(
            &self,
            open_try: MsgChannelOpenTry,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([50, 157, 243, 145], (open_try,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `client_create` (0x184c14a5) function"]
        pub fn client_create(
            &self,
            create: MsgClientCreate,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([24, 76, 20, 165], (create,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `client_misbehaviour` (0x25f48ab6) function"]
        pub fn client_misbehaviour(
            &self,
            misbehaviour: MsgClientMisbehaviour,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([37, 244, 138, 182], (misbehaviour,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `client_update` (0x58d9f0b4) function"]
        pub fn client_update(
            &self,
            update: MsgClientUpdate,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([88, 217, 240, 180], (update,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `close_packet` (0x02bc9c31) function"]
        pub fn close_packet(
            &self,
            close_packet: MsgTimeoutPacket,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([2, 188, 156, 49], (close_packet,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `connection_open_ack` (0xadabd8f9) function"]
        pub fn connection_open_ack(
            &self,
            open_ack: MsgConnectionOpenAck,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([173, 171, 216, 249], (open_ack,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `connection_open_confirm` (0xe7227e70) function"]
        pub fn connection_open_confirm(
            &self,
            open_confirm: MsgConnectionOpenConfirm,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([231, 34, 126, 112], (open_confirm,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `connection_open_init` (0x7e449d16) function"]
        pub fn connection_open_init(
            &self,
            open_init: MsgConnectionOpenInit,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([126, 68, 157, 22], (open_init,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `connection_open_try` (0xb3465fd9) function"]
        pub fn connection_open_try(
            &self,
            open_try: MsgConnectionOpenTry,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([179, 70, 95, 217], (open_try,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `construct` (0x94b91deb) function"]
        pub fn construct(&self) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([148, 185, 29, 235], ())
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `getRoleAdmin` (0x248a9ca3) function"]
        pub fn get_role_admin(
            &self,
            role: [u8; 32],
        ) -> ethers::contract::builders::ContractCall<M, [u8; 32]> {
            self.0
                .method_hash([36, 138, 156, 163], role)
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `get_light_client` (0x9a356bba) function"]
        pub fn get_light_client(
            &self,
            client_id: String,
        ) -> ethers::contract::builders::ContractCall<M, ethers::core::types::Address> {
            self.0
                .method_hash([154, 53, 107, 186], client_id)
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `grantRole` (0x2f2ff15d) function"]
        pub fn grant_role(
            &self,
            role: [u8; 32],
            account: ethers::core::types::Address,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([47, 47, 241, 93], (role, account))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `hasRole` (0x91d14854) function"]
        pub fn has_role(
            &self,
            role: [u8; 32],
            account: ethers::core::types::Address,
        ) -> ethers::contract::builders::ContractCall<M, bool> {
            self.0
                .method_hash([145, 209, 72, 84], (role, account))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `recv_packet` (0x9916d69b) function"]
        pub fn recv_packet(
            &self,
            recv_packet: MsgRecvPacket,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([153, 22, 214, 155], (recv_packet,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `renounceRole` (0x36568abe) function"]
        pub fn renounce_role(
            &self,
            role: [u8; 32],
            account: ethers::core::types::Address,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([54, 86, 138, 190], (role, account))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `revokeRole` (0xd547741f) function"]
        pub fn revoke_role(
            &self,
            role: [u8; 32],
            account: ethers::core::types::Address,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([213, 71, 116, 31], (role, account))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `set_light_client` (0xcd3c84ec) function"]
        pub fn set_light_client(
            &self,
            client_type: u8,
            light_client: ethers::core::types::Address,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([205, 60, 132, 236], (client_type, light_client))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `set_pause` (0x613bd3aa) function"]
        pub fn set_pause(&self, pause: bool) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([97, 59, 211, 170], pause)
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `supportsInterface` (0x01ffc9a7) function"]
        pub fn supports_interface(
            &self,
            interface_id: [u8; 4],
        ) -> ethers::contract::builders::ContractCall<M, bool> {
            self.0
                .method_hash([1, 255, 201, 167], interface_id)
                .expect("method not found (this should never happen)")
        }
        #[doc = "Calls the contract's `timeout_packet` (0xc449903b) function"]
        pub fn timeout_packet(
            &self,
            timeout_packet: MsgTimeoutPacket,
        ) -> ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([196, 73, 144, 59], (timeout_packet,))
                .expect("method not found (this should never happen)")
        }
        #[doc = "Gets the contract's `CreateClient` event"]
        pub fn create_client_filter(
            &self,
        ) -> ethers::contract::builders::Event<M, CreateClientFilter> {
            self.0.event()
        }
        #[doc = "Gets the contract's `RoleAdminChanged` event"]
        pub fn role_admin_changed_filter(
            &self,
        ) -> ethers::contract::builders::Event<M, RoleAdminChangedFilter> {
            self.0.event()
        }
        #[doc = "Gets the contract's `RoleGranted` event"]
        pub fn role_granted_filter(
            &self,
        ) -> ethers::contract::builders::Event<M, RoleGrantedFilter> {
            self.0.event()
        }
        #[doc = "Gets the contract's `RoleRevoked` event"]
        pub fn role_revoked_filter(
            &self,
        ) -> ethers::contract::builders::Event<M, RoleRevokedFilter> {
            self.0.event()
        }
        #[doc = "Gets the contract's `UpdateClient` event"]
        pub fn update_client_filter(
            &self,
        ) -> ethers::contract::builders::Event<M, UpdateClientFilter> {
            self.0.event()
        }
        #[doc = r" Returns an [`Event`](#ethers_contract::builders::Event) builder for all events of this contract"]
        pub fn events(&self) -> ethers::contract::builders::Event<M, IBCEvents> {
            self.0.event_with_filter(Default::default())
        }
    }
    impl<M: ethers::providers::Middleware> From<ethers::contract::Contract<M>> for IBC<M> {
        fn from(contract: ethers::contract::Contract<M>) -> Self {
            Self(contract)
        }
    }
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthEvent,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethevent(name = "CreateClient", abi = "CreateClient(string,uint256,uint256)")]
    pub struct CreateClientFilter {
        pub client_id: String,
        #[ethevent(indexed)]
        pub client_type: ethers::core::types::U256,
        pub number: ethers::core::types::U256,
    }
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthEvent,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethevent(
        name = "RoleAdminChanged",
        abi = "RoleAdminChanged(bytes32,bytes32,bytes32)"
    )]
    pub struct RoleAdminChangedFilter {
        #[ethevent(indexed)]
        pub role: [u8; 32],
        #[ethevent(indexed)]
        pub previous_admin_role: [u8; 32],
        #[ethevent(indexed)]
        pub new_admin_role: [u8; 32],
    }
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthEvent,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethevent(name = "RoleGranted", abi = "RoleGranted(bytes32,address,address)")]
    pub struct RoleGrantedFilter {
        #[ethevent(indexed)]
        pub role: [u8; 32],
        #[ethevent(indexed)]
        pub account: ethers::core::types::Address,
        #[ethevent(indexed)]
        pub sender: ethers::core::types::Address,
    }
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthEvent,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethevent(name = "RoleRevoked", abi = "RoleRevoked(bytes32,address,address)")]
    pub struct RoleRevokedFilter {
        #[ethevent(indexed)]
        pub role: [u8; 32],
        #[ethevent(indexed)]
        pub account: ethers::core::types::Address,
        #[ethevent(indexed)]
        pub sender: ethers::core::types::Address,
    }
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthEvent,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethevent(name = "UpdateClient", abi = "UpdateClient(string,uint256,uint256)")]
    pub struct UpdateClientFilter {
        pub client_id: String,
        #[ethevent(indexed)]
        pub client_type: ethers::core::types::U256,
        pub number: ethers::core::types::U256,
    }
    #[derive(Debug, Clone, PartialEq, Eq, ethers :: contract :: EthAbiType)]
    pub enum IBCEvents {
        CreateClientFilter(CreateClientFilter),
        RoleAdminChangedFilter(RoleAdminChangedFilter),
        RoleGrantedFilter(RoleGrantedFilter),
        RoleRevokedFilter(RoleRevokedFilter),
        UpdateClientFilter(UpdateClientFilter),
    }
    impl ethers::contract::EthLogDecode for IBCEvents {
        fn decode_log(
            log: &ethers::core::abi::RawLog,
        ) -> ::std::result::Result<Self, ethers::core::abi::Error>
        where
            Self: Sized,
        {
            if let Ok(decoded) = CreateClientFilter::decode_log(log) {
                return Ok(IBCEvents::CreateClientFilter(decoded));
            }
            if let Ok(decoded) = RoleAdminChangedFilter::decode_log(log) {
                return Ok(IBCEvents::RoleAdminChangedFilter(decoded));
            }
            if let Ok(decoded) = RoleGrantedFilter::decode_log(log) {
                return Ok(IBCEvents::RoleGrantedFilter(decoded));
            }
            if let Ok(decoded) = RoleRevokedFilter::decode_log(log) {
                return Ok(IBCEvents::RoleRevokedFilter(decoded));
            }
            if let Ok(decoded) = UpdateClientFilter::decode_log(log) {
                return Ok(IBCEvents::UpdateClientFilter(decoded));
            }
            Err(ethers::core::abi::Error::InvalidData)
        }
    }
    impl ::std::fmt::Display for IBCEvents {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match self {
                IBCEvents::CreateClientFilter(element) => element.fmt(f),
                IBCEvents::RoleAdminChangedFilter(element) => element.fmt(f),
                IBCEvents::RoleGrantedFilter(element) => element.fmt(f),
                IBCEvents::RoleRevokedFilter(element) => element.fmt(f),
                IBCEvents::UpdateClientFilter(element) => element.fmt(f),
            }
        }
    }
    #[doc = "Container type for all input parameters for the `DEFAULT_ADMIN_ROLE` function with signature `DEFAULT_ADMIN_ROLE()` and selector `[162, 23, 253, 223]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "DEFAULT_ADMIN_ROLE", abi = "DEFAULT_ADMIN_ROLE()")]
    pub struct DefaultAdminRoleCall;
    #[doc = "Container type for all input parameters for the `IBC_RELAYER` function with signature `IBC_RELAYER()` and selector `[192, 45, 94, 85]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "IBC_RELAYER", abi = "IBC_RELAYER()")]
    pub struct IbcRelayerCall;
    #[doc = "Container type for all input parameters for the `ack_packet` function with signature `ack_packet(((uint256,string,string,string,string,bytes,bytes32,uint256),bytes,(uint256,bytes,bytes,bytes,bytes)))` and selector `[170, 45, 142, 253]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "ack_packet",
        abi = "ack_packet(((uint256,string,string,string,string,bytes,bytes32,uint256),bytes,(uint256,bytes,bytes,bytes,bytes)))"
    )]
    pub struct AckPacketCall {
        pub ack_packet: MsgAckPacket,
    }
    #[doc = "Container type for all input parameters for the `channel_close_confirm` function with signature `channel_close_confirm((string,(string,string),(uint256,bytes,bytes,bytes,bytes)))` and selector `[34, 116, 74, 95]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "channel_close_confirm",
        abi = "channel_close_confirm((string,(string,string),(uint256,bytes,bytes,bytes,bytes)))"
    )]
    pub struct ChannelCloseConfirmCall {
        pub close_confirm: MsgChannelCloseConfirm,
    }
    #[doc = "Container type for all input parameters for the `channel_close_init` function with signature `channel_close_init((string,(string,string)))` and selector `[140, 186, 146, 69]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "channel_close_init",
        abi = "channel_close_init((string,(string,string)))"
    )]
    pub struct ChannelCloseInitCall {
        pub close_init: MsgChannelCloseInit,
    }
    #[doc = "Container type for all input parameters for the `channel_open_ack` function with signature `channel_open_ack((string,(string,string),(string,string),string,(uint256,bytes,bytes,bytes,bytes)))` and selector `[234, 221, 235, 124]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "channel_open_ack",
        abi = "channel_open_ack((string,(string,string),(string,string),string,(uint256,bytes,bytes,bytes,bytes)))"
    )]
    pub struct ChannelOpenAckCall {
        pub open_ack: MsgChannelOpenAck,
    }
    #[doc = "Container type for all input parameters for the `channel_open_confirm` function with signature `channel_open_confirm((string,(string,string),(uint256,bytes,bytes,bytes,bytes)))` and selector `[45, 42, 184, 180]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "channel_open_confirm",
        abi = "channel_open_confirm((string,(string,string),(uint256,bytes,bytes,bytes,bytes)))"
    )]
    pub struct ChannelOpenConfirmCall {
        pub open_confirm: MsgChannelOpenConfirm,
    }
    #[doc = "Container type for all input parameters for the `channel_open_init` function with signature `channel_open_init((string,((string,string),uint8,uint8,(string,string),string[],string)))` and selector `[193, 226, 133, 74]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "channel_open_init",
        abi = "channel_open_init((string,((string,string),uint8,uint8,(string,string),string[],string)))"
    )]
    pub struct ChannelOpenInitCall {
        pub open_init: MsgChannelOpenInit,
    }
    #[doc = "Container type for all input parameters for the `channel_open_try` function with signature `channel_open_try((string,(string,string),((string,string),uint8,uint8,(string,string),string[],string),string,(uint256,bytes,bytes,bytes,bytes)))` and selector `[50, 157, 243, 145]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "channel_open_try",
        abi = "channel_open_try((string,(string,string),((string,string),uint8,uint8,(string,string),string[],string),string,(uint256,bytes,bytes,bytes,bytes)))"
    )]
    pub struct ChannelOpenTryCall {
        pub open_try: MsgChannelOpenTry,
    }
    #[doc = "Container type for all input parameters for the `client_create` function with signature `client_create(((string,uint8,bytes32,bytes32,uint256,uint256,bytes),(uint256,bytes32,bytes)))` and selector `[24, 76, 20, 165]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "client_create",
        abi = "client_create(((string,uint8,bytes32,bytes32,uint256,uint256,bytes),(uint256,bytes32,bytes)))"
    )]
    pub struct ClientCreateCall {
        pub create: MsgClientCreate,
    }
    #[doc = "Container type for all input parameters for the `client_misbehaviour` function with signature `client_misbehaviour((string,bytes,bytes))` and selector `[37, 244, 138, 182]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "client_misbehaviour",
        abi = "client_misbehaviour((string,bytes,bytes))"
    )]
    pub struct ClientMisbehaviourCall {
        pub misbehaviour: MsgClientMisbehaviour,
    }
    #[doc = "Container type for all input parameters for the `client_update` function with signature `client_update((string,bytes))` and selector `[88, 217, 240, 180]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "client_update", abi = "client_update((string,bytes))")]
    pub struct ClientUpdateCall {
        pub update: MsgClientUpdate,
    }
    #[doc = "Container type for all input parameters for the `close_packet` function with signature `close_packet(((uint256,string,string,string,string,bytes,bytes32,uint256),uint256,(uint256,bytes,bytes,bytes,bytes)))` and selector `[2, 188, 156, 49]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "close_packet",
        abi = "close_packet(((uint256,string,string,string,string,bytes,bytes32,uint256),uint256,(uint256,bytes,bytes,bytes,bytes)))"
    )]
    pub struct ClosePacketCall {
        pub close_packet: MsgTimeoutPacket,
    }
    #[doc = "Container type for all input parameters for the `connection_open_ack` function with signature `connection_open_ack((string,string,(string,uint8,bytes32,bytes32,uint256,uint256,bytes),(uint256,bytes,bytes,bytes,bytes),string))` and selector `[173, 171, 216, 249]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "connection_open_ack",
        abi = "connection_open_ack((string,string,(string,uint8,bytes32,bytes32,uint256,uint256,bytes),(uint256,bytes,bytes,bytes,bytes),string))"
    )]
    pub struct ConnectionOpenAckCall {
        pub open_ack: MsgConnectionOpenAck,
    }
    #[doc = "Container type for all input parameters for the `connection_open_confirm` function with signature `connection_open_confirm((string,(uint256,bytes,bytes,bytes,bytes)))` and selector `[231, 34, 126, 112]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "connection_open_confirm",
        abi = "connection_open_confirm((string,(uint256,bytes,bytes,bytes,bytes)))"
    )]
    pub struct ConnectionOpenConfirmCall {
        pub open_confirm: MsgConnectionOpenConfirm,
    }
    #[doc = "Container type for all input parameters for the `connection_open_init` function with signature `connection_open_init((string,(string,string,bytes),string,uint256))` and selector `[126, 68, 157, 22]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "connection_open_init",
        abi = "connection_open_init((string,(string,string,bytes),string,uint256))"
    )]
    pub struct ConnectionOpenInitCall {
        pub open_init: MsgConnectionOpenInit,
    }
    #[doc = "Container type for all input parameters for the `connection_open_try` function with signature `connection_open_try((string,string,(string,uint8,bytes32,bytes32,uint256,uint256,bytes),(string,string,bytes),string[],(uint256,bytes,bytes,bytes,bytes),uint256))` and selector `[179, 70, 95, 217]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "connection_open_try",
        abi = "connection_open_try((string,string,(string,uint8,bytes32,bytes32,uint256,uint256,bytes),(string,string,bytes),string[],(uint256,bytes,bytes,bytes,bytes),uint256))"
    )]
    pub struct ConnectionOpenTryCall {
        pub open_try: MsgConnectionOpenTry,
    }
    #[doc = "Container type for all input parameters for the `construct` function with signature `construct()` and selector `[148, 185, 29, 235]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "construct", abi = "construct()")]
    pub struct ConstructCall;
    #[doc = "Container type for all input parameters for the `getRoleAdmin` function with signature `getRoleAdmin(bytes32)` and selector `[36, 138, 156, 163]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "getRoleAdmin", abi = "getRoleAdmin(bytes32)")]
    pub struct GetRoleAdminCall {
        pub role: [u8; 32],
    }
    #[doc = "Container type for all input parameters for the `get_light_client` function with signature `get_light_client(string)` and selector `[154, 53, 107, 186]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "get_light_client", abi = "get_light_client(string)")]
    pub struct GetLightClientCall {
        pub client_id: String,
    }
    #[doc = "Container type for all input parameters for the `grantRole` function with signature `grantRole(bytes32,address)` and selector `[47, 47, 241, 93]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "grantRole", abi = "grantRole(bytes32,address)")]
    pub struct GrantRoleCall {
        pub role: [u8; 32],
        pub account: ethers::core::types::Address,
    }
    #[doc = "Container type for all input parameters for the `hasRole` function with signature `hasRole(bytes32,address)` and selector `[145, 209, 72, 84]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "hasRole", abi = "hasRole(bytes32,address)")]
    pub struct HasRoleCall {
        pub role: [u8; 32],
        pub account: ethers::core::types::Address,
    }
    #[doc = "Container type for all input parameters for the `recv_packet` function with signature `recv_packet(((uint256,string,string,string,string,bytes,bytes32,uint256),(uint256,bytes,bytes,bytes,bytes)))` and selector `[153, 22, 214, 155]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "recv_packet",
        abi = "recv_packet(((uint256,string,string,string,string,bytes,bytes32,uint256),(uint256,bytes,bytes,bytes,bytes)))"
    )]
    pub struct RecvPacketCall {
        pub recv_packet: MsgRecvPacket,
    }
    #[doc = "Container type for all input parameters for the `renounceRole` function with signature `renounceRole(bytes32,address)` and selector `[54, 86, 138, 190]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "renounceRole", abi = "renounceRole(bytes32,address)")]
    pub struct RenounceRoleCall {
        pub role: [u8; 32],
        pub account: ethers::core::types::Address,
    }
    #[doc = "Container type for all input parameters for the `revokeRole` function with signature `revokeRole(bytes32,address)` and selector `[213, 71, 116, 31]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "revokeRole", abi = "revokeRole(bytes32,address)")]
    pub struct RevokeRoleCall {
        pub role: [u8; 32],
        pub account: ethers::core::types::Address,
    }
    #[doc = "Container type for all input parameters for the `set_light_client` function with signature `set_light_client(uint8,address)` and selector `[205, 60, 132, 236]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "set_light_client", abi = "set_light_client(uint8,address)")]
    pub struct SetLightClientCall {
        pub client_type: u8,
        pub light_client: ethers::core::types::Address,
    }
    #[doc = "Container type for all input parameters for the `set_pause` function with signature `set_pause(bool)` and selector `[97, 59, 211, 170]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "set_pause", abi = "set_pause(bool)")]
    pub struct SetPauseCall {
        pub pause: bool,
    }
    #[doc = "Container type for all input parameters for the `supportsInterface` function with signature `supportsInterface(bytes4)` and selector `[1, 255, 201, 167]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(name = "supportsInterface", abi = "supportsInterface(bytes4)")]
    pub struct SupportsInterfaceCall {
        pub interface_id: [u8; 4],
    }
    #[doc = "Container type for all input parameters for the `timeout_packet` function with signature `timeout_packet(((uint256,string,string,string,string,bytes,bytes32,uint256),uint256,(uint256,bytes,bytes,bytes,bytes)))` and selector `[196, 73, 144, 59]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthCall,
        ethers :: contract :: EthDisplay,
        Default,
    )]
    #[ethcall(
        name = "timeout_packet",
        abi = "timeout_packet(((uint256,string,string,string,string,bytes,bytes32,uint256),uint256,(uint256,bytes,bytes,bytes,bytes)))"
    )]
    pub struct TimeoutPacketCall {
        pub timeout_packet: MsgTimeoutPacket,
    }
    #[derive(Debug, Clone, PartialEq, Eq, ethers :: contract :: EthAbiType)]
    pub enum IBCCalls {
        DefaultAdminRole(DefaultAdminRoleCall),
        IbcRelayer(IbcRelayerCall),
        AckPacket(AckPacketCall),
        ChannelCloseConfirm(ChannelCloseConfirmCall),
        ChannelCloseInit(ChannelCloseInitCall),
        ChannelOpenAck(ChannelOpenAckCall),
        ChannelOpenConfirm(ChannelOpenConfirmCall),
        ChannelOpenInit(ChannelOpenInitCall),
        ChannelOpenTry(ChannelOpenTryCall),
        ClientCreate(ClientCreateCall),
        ClientMisbehaviour(ClientMisbehaviourCall),
        ClientUpdate(ClientUpdateCall),
        ClosePacket(ClosePacketCall),
        ConnectionOpenAck(ConnectionOpenAckCall),
        ConnectionOpenConfirm(ConnectionOpenConfirmCall),
        ConnectionOpenInit(ConnectionOpenInitCall),
        ConnectionOpenTry(ConnectionOpenTryCall),
        Construct(ConstructCall),
        GetRoleAdmin(GetRoleAdminCall),
        GetLightClient(GetLightClientCall),
        GrantRole(GrantRoleCall),
        HasRole(HasRoleCall),
        RecvPacket(RecvPacketCall),
        RenounceRole(RenounceRoleCall),
        RevokeRole(RevokeRoleCall),
        SetLightClient(SetLightClientCall),
        SetPause(SetPauseCall),
        SupportsInterface(SupportsInterfaceCall),
        TimeoutPacket(TimeoutPacketCall),
    }
    impl ethers::core::abi::AbiDecode for IBCCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::std::result::Result<Self, ethers::core::abi::AbiError> {
            if let Ok(decoded) =
                <DefaultAdminRoleCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::DefaultAdminRole(decoded));
            }
            if let Ok(decoded) =
                <IbcRelayerCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::IbcRelayer(decoded));
            }
            if let Ok(decoded) =
                <AckPacketCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::AckPacket(decoded));
            }
            if let Ok(decoded) =
                <ChannelCloseConfirmCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ChannelCloseConfirm(decoded));
            }
            if let Ok(decoded) =
                <ChannelCloseInitCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ChannelCloseInit(decoded));
            }
            if let Ok(decoded) =
                <ChannelOpenAckCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ChannelOpenAck(decoded));
            }
            if let Ok(decoded) =
                <ChannelOpenConfirmCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ChannelOpenConfirm(decoded));
            }
            if let Ok(decoded) =
                <ChannelOpenInitCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ChannelOpenInit(decoded));
            }
            if let Ok(decoded) =
                <ChannelOpenTryCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ChannelOpenTry(decoded));
            }
            if let Ok(decoded) =
                <ClientCreateCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ClientCreate(decoded));
            }
            if let Ok(decoded) =
                <ClientMisbehaviourCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ClientMisbehaviour(decoded));
            }
            if let Ok(decoded) =
                <ClientUpdateCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ClientUpdate(decoded));
            }
            if let Ok(decoded) =
                <ClosePacketCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ClosePacket(decoded));
            }
            if let Ok(decoded) =
                <ConnectionOpenAckCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ConnectionOpenAck(decoded));
            }
            if let Ok(decoded) =
                <ConnectionOpenConfirmCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ConnectionOpenConfirm(decoded));
            }
            if let Ok(decoded) =
                <ConnectionOpenInitCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ConnectionOpenInit(decoded));
            }
            if let Ok(decoded) =
                <ConnectionOpenTryCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::ConnectionOpenTry(decoded));
            }
            if let Ok(decoded) =
                <ConstructCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::Construct(decoded));
            }
            if let Ok(decoded) =
                <GetRoleAdminCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::GetRoleAdmin(decoded));
            }
            if let Ok(decoded) =
                <GetLightClientCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::GetLightClient(decoded));
            }
            if let Ok(decoded) =
                <GrantRoleCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::GrantRole(decoded));
            }
            if let Ok(decoded) =
                <HasRoleCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::HasRole(decoded));
            }
            if let Ok(decoded) =
                <RecvPacketCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::RecvPacket(decoded));
            }
            if let Ok(decoded) =
                <RenounceRoleCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::RenounceRole(decoded));
            }
            if let Ok(decoded) =
                <RevokeRoleCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::RevokeRole(decoded));
            }
            if let Ok(decoded) =
                <SetLightClientCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::SetLightClient(decoded));
            }
            if let Ok(decoded) =
                <SetPauseCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::SetPause(decoded));
            }
            if let Ok(decoded) =
                <SupportsInterfaceCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::SupportsInterface(decoded));
            }
            if let Ok(decoded) =
                <TimeoutPacketCall as ethers::core::abi::AbiDecode>::decode(data.as_ref())
            {
                return Ok(IBCCalls::TimeoutPacket(decoded));
            }
            Err(ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ethers::core::abi::AbiEncode for IBCCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                IBCCalls::DefaultAdminRole(element) => element.encode(),
                IBCCalls::IbcRelayer(element) => element.encode(),
                IBCCalls::AckPacket(element) => element.encode(),
                IBCCalls::ChannelCloseConfirm(element) => element.encode(),
                IBCCalls::ChannelCloseInit(element) => element.encode(),
                IBCCalls::ChannelOpenAck(element) => element.encode(),
                IBCCalls::ChannelOpenConfirm(element) => element.encode(),
                IBCCalls::ChannelOpenInit(element) => element.encode(),
                IBCCalls::ChannelOpenTry(element) => element.encode(),
                IBCCalls::ClientCreate(element) => element.encode(),
                IBCCalls::ClientMisbehaviour(element) => element.encode(),
                IBCCalls::ClientUpdate(element) => element.encode(),
                IBCCalls::ClosePacket(element) => element.encode(),
                IBCCalls::ConnectionOpenAck(element) => element.encode(),
                IBCCalls::ConnectionOpenConfirm(element) => element.encode(),
                IBCCalls::ConnectionOpenInit(element) => element.encode(),
                IBCCalls::ConnectionOpenTry(element) => element.encode(),
                IBCCalls::Construct(element) => element.encode(),
                IBCCalls::GetRoleAdmin(element) => element.encode(),
                IBCCalls::GetLightClient(element) => element.encode(),
                IBCCalls::GrantRole(element) => element.encode(),
                IBCCalls::HasRole(element) => element.encode(),
                IBCCalls::RecvPacket(element) => element.encode(),
                IBCCalls::RenounceRole(element) => element.encode(),
                IBCCalls::RevokeRole(element) => element.encode(),
                IBCCalls::SetLightClient(element) => element.encode(),
                IBCCalls::SetPause(element) => element.encode(),
                IBCCalls::SupportsInterface(element) => element.encode(),
                IBCCalls::TimeoutPacket(element) => element.encode(),
            }
        }
    }
    impl ::std::fmt::Display for IBCCalls {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match self {
                IBCCalls::DefaultAdminRole(element) => element.fmt(f),
                IBCCalls::IbcRelayer(element) => element.fmt(f),
                IBCCalls::AckPacket(element) => element.fmt(f),
                IBCCalls::ChannelCloseConfirm(element) => element.fmt(f),
                IBCCalls::ChannelCloseInit(element) => element.fmt(f),
                IBCCalls::ChannelOpenAck(element) => element.fmt(f),
                IBCCalls::ChannelOpenConfirm(element) => element.fmt(f),
                IBCCalls::ChannelOpenInit(element) => element.fmt(f),
                IBCCalls::ChannelOpenTry(element) => element.fmt(f),
                IBCCalls::ClientCreate(element) => element.fmt(f),
                IBCCalls::ClientMisbehaviour(element) => element.fmt(f),
                IBCCalls::ClientUpdate(element) => element.fmt(f),
                IBCCalls::ClosePacket(element) => element.fmt(f),
                IBCCalls::ConnectionOpenAck(element) => element.fmt(f),
                IBCCalls::ConnectionOpenConfirm(element) => element.fmt(f),
                IBCCalls::ConnectionOpenInit(element) => element.fmt(f),
                IBCCalls::ConnectionOpenTry(element) => element.fmt(f),
                IBCCalls::Construct(element) => element.fmt(f),
                IBCCalls::GetRoleAdmin(element) => element.fmt(f),
                IBCCalls::GetLightClient(element) => element.fmt(f),
                IBCCalls::GrantRole(element) => element.fmt(f),
                IBCCalls::HasRole(element) => element.fmt(f),
                IBCCalls::RecvPacket(element) => element.fmt(f),
                IBCCalls::RenounceRole(element) => element.fmt(f),
                IBCCalls::RevokeRole(element) => element.fmt(f),
                IBCCalls::SetLightClient(element) => element.fmt(f),
                IBCCalls::SetPause(element) => element.fmt(f),
                IBCCalls::SupportsInterface(element) => element.fmt(f),
                IBCCalls::TimeoutPacket(element) => element.fmt(f),
            }
        }
    }
    impl ::std::convert::From<DefaultAdminRoleCall> for IBCCalls {
        fn from(var: DefaultAdminRoleCall) -> Self {
            IBCCalls::DefaultAdminRole(var)
        }
    }
    impl ::std::convert::From<IbcRelayerCall> for IBCCalls {
        fn from(var: IbcRelayerCall) -> Self {
            IBCCalls::IbcRelayer(var)
        }
    }
    impl ::std::convert::From<AckPacketCall> for IBCCalls {
        fn from(var: AckPacketCall) -> Self {
            IBCCalls::AckPacket(var)
        }
    }
    impl ::std::convert::From<ChannelCloseConfirmCall> for IBCCalls {
        fn from(var: ChannelCloseConfirmCall) -> Self {
            IBCCalls::ChannelCloseConfirm(var)
        }
    }
    impl ::std::convert::From<ChannelCloseInitCall> for IBCCalls {
        fn from(var: ChannelCloseInitCall) -> Self {
            IBCCalls::ChannelCloseInit(var)
        }
    }
    impl ::std::convert::From<ChannelOpenAckCall> for IBCCalls {
        fn from(var: ChannelOpenAckCall) -> Self {
            IBCCalls::ChannelOpenAck(var)
        }
    }
    impl ::std::convert::From<ChannelOpenConfirmCall> for IBCCalls {
        fn from(var: ChannelOpenConfirmCall) -> Self {
            IBCCalls::ChannelOpenConfirm(var)
        }
    }
    impl ::std::convert::From<ChannelOpenInitCall> for IBCCalls {
        fn from(var: ChannelOpenInitCall) -> Self {
            IBCCalls::ChannelOpenInit(var)
        }
    }
    impl ::std::convert::From<ChannelOpenTryCall> for IBCCalls {
        fn from(var: ChannelOpenTryCall) -> Self {
            IBCCalls::ChannelOpenTry(var)
        }
    }
    impl ::std::convert::From<ClientCreateCall> for IBCCalls {
        fn from(var: ClientCreateCall) -> Self {
            IBCCalls::ClientCreate(var)
        }
    }
    impl ::std::convert::From<ClientMisbehaviourCall> for IBCCalls {
        fn from(var: ClientMisbehaviourCall) -> Self {
            IBCCalls::ClientMisbehaviour(var)
        }
    }
    impl ::std::convert::From<ClientUpdateCall> for IBCCalls {
        fn from(var: ClientUpdateCall) -> Self {
            IBCCalls::ClientUpdate(var)
        }
    }
    impl ::std::convert::From<ClosePacketCall> for IBCCalls {
        fn from(var: ClosePacketCall) -> Self {
            IBCCalls::ClosePacket(var)
        }
    }
    impl ::std::convert::From<ConnectionOpenAckCall> for IBCCalls {
        fn from(var: ConnectionOpenAckCall) -> Self {
            IBCCalls::ConnectionOpenAck(var)
        }
    }
    impl ::std::convert::From<ConnectionOpenConfirmCall> for IBCCalls {
        fn from(var: ConnectionOpenConfirmCall) -> Self {
            IBCCalls::ConnectionOpenConfirm(var)
        }
    }
    impl ::std::convert::From<ConnectionOpenInitCall> for IBCCalls {
        fn from(var: ConnectionOpenInitCall) -> Self {
            IBCCalls::ConnectionOpenInit(var)
        }
    }
    impl ::std::convert::From<ConnectionOpenTryCall> for IBCCalls {
        fn from(var: ConnectionOpenTryCall) -> Self {
            IBCCalls::ConnectionOpenTry(var)
        }
    }
    impl ::std::convert::From<ConstructCall> for IBCCalls {
        fn from(var: ConstructCall) -> Self {
            IBCCalls::Construct(var)
        }
    }
    impl ::std::convert::From<GetRoleAdminCall> for IBCCalls {
        fn from(var: GetRoleAdminCall) -> Self {
            IBCCalls::GetRoleAdmin(var)
        }
    }
    impl ::std::convert::From<GetLightClientCall> for IBCCalls {
        fn from(var: GetLightClientCall) -> Self {
            IBCCalls::GetLightClient(var)
        }
    }
    impl ::std::convert::From<GrantRoleCall> for IBCCalls {
        fn from(var: GrantRoleCall) -> Self {
            IBCCalls::GrantRole(var)
        }
    }
    impl ::std::convert::From<HasRoleCall> for IBCCalls {
        fn from(var: HasRoleCall) -> Self {
            IBCCalls::HasRole(var)
        }
    }
    impl ::std::convert::From<RecvPacketCall> for IBCCalls {
        fn from(var: RecvPacketCall) -> Self {
            IBCCalls::RecvPacket(var)
        }
    }
    impl ::std::convert::From<RenounceRoleCall> for IBCCalls {
        fn from(var: RenounceRoleCall) -> Self {
            IBCCalls::RenounceRole(var)
        }
    }
    impl ::std::convert::From<RevokeRoleCall> for IBCCalls {
        fn from(var: RevokeRoleCall) -> Self {
            IBCCalls::RevokeRole(var)
        }
    }
    impl ::std::convert::From<SetLightClientCall> for IBCCalls {
        fn from(var: SetLightClientCall) -> Self {
            IBCCalls::SetLightClient(var)
        }
    }
    impl ::std::convert::From<SetPauseCall> for IBCCalls {
        fn from(var: SetPauseCall) -> Self {
            IBCCalls::SetPause(var)
        }
    }
    impl ::std::convert::From<SupportsInterfaceCall> for IBCCalls {
        fn from(var: SupportsInterfaceCall) -> Self {
            IBCCalls::SupportsInterface(var)
        }
    }
    impl ::std::convert::From<TimeoutPacketCall> for IBCCalls {
        fn from(var: TimeoutPacketCall) -> Self {
            IBCCalls::TimeoutPacket(var)
        }
    }
    #[doc = "Container type for all return fields from the `DEFAULT_ADMIN_ROLE` function with signature `DEFAULT_ADMIN_ROLE()` and selector `[162, 23, 253, 223]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
        Default,
    )]
    pub struct DefaultAdminRoleReturn(pub [u8; 32]);
    #[doc = "Container type for all return fields from the `IBC_RELAYER` function with signature `IBC_RELAYER()` and selector `[192, 45, 94, 85]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
        Default,
    )]
    pub struct IbcRelayerReturn(pub [u8; 32]);
    #[doc = "Container type for all return fields from the `getRoleAdmin` function with signature `getRoleAdmin(bytes32)` and selector `[36, 138, 156, 163]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
        Default,
    )]
    pub struct GetRoleAdminReturn(pub [u8; 32]);
    #[doc = "Container type for all return fields from the `get_light_client` function with signature `get_light_client(string)` and selector `[154, 53, 107, 186]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
        Default,
    )]
    pub struct GetLightClientReturn(pub ethers::core::types::Address);
    #[doc = "Container type for all return fields from the `hasRole` function with signature `hasRole(bytes32,address)` and selector `[145, 209, 72, 84]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
        Default,
    )]
    pub struct HasRoleReturn(pub bool);
    #[doc = "Container type for all return fields from the `supportsInterface` function with signature `supportsInterface(bytes4)` and selector `[1, 255, 201, 167]`"]
    #[derive(
        Clone,
        Debug,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
        Default,
    )]
    pub struct SupportsInterfaceReturn(pub bool);
    #[doc = "`ChannelEnd((string,string),uint8,uint8,(string,string),string[],string)`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct ChannelEnd {
        pub channel_id: ChannelId,
        pub state: u8,
        pub odering: u8,
        pub remote: ChannelId,
        pub connection_hops: Vec<String>,
        pub version: String,
    }
    #[doc = "`ChannelId(string,string)`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct ChannelId {
        pub port_id: String,
        pub channel_id: String,
    }
    #[doc = "`ClientState(string,uint8,bytes32,bytes32,uint256,uint256,bytes)`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct ClientState {
        pub chain_id: String,
        pub client_type: u8,
        pub latest_height: [u8; 32],
        pub frozen_height: [u8; 32],
        pub trusting_period: ethers::core::types::U256,
        pub max_clock_drift: ethers::core::types::U256,
        pub extra_payload: ethers::core::types::Bytes,
    }
    #[doc = "`ConnectionId(string,string,bytes)`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct ConnectionId {
        pub client_id: String,
        pub connection_id: String,
        pub commitment_prefix: ethers::core::types::Bytes,
    }
    #[doc = "`ConsensusState(uint256,bytes32,bytes)`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct ConsensusState {
        pub timestamp: ethers::core::types::U256,
        pub commitment_root: [u8; 32],
        pub extra_payload: ethers::core::types::Bytes,
    }
    #[doc = "`MsgAckPacket((uint256,string,string,string,string,bytes,bytes32,uint256),bytes,(uint256,bytes,bytes,bytes,bytes))`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgAckPacket {
        pub packet: Packet,
        pub acknowledgement: ethers::core::types::Bytes,
        pub proofs: Proofs,
    }
    #[doc = "`MsgChannelCloseConfirm(string,(string,string),(uint256,bytes,bytes,bytes,bytes))`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgChannelCloseConfirm {
        pub port_id: String,
        pub channel_id: ChannelId,
        pub proofs: Proofs,
    }
    #[doc = "`MsgChannelCloseInit(string,(string,string))`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgChannelCloseInit {
        pub port_id: String,
        pub channel_id: ChannelId,
    }
    #[doc = "`MsgChannelOpenAck(string,(string,string),(string,string),string,(uint256,bytes,bytes,bytes,bytes))`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgChannelOpenAck {
        pub port_id: String,
        pub channel_id: ChannelId,
        pub counterparty_channel_id: ChannelId,
        pub counterparty_version: String,
        pub proofs: Proofs,
    }
    #[doc = "`MsgChannelOpenConfirm(string,(string,string),(uint256,bytes,bytes,bytes,bytes))`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgChannelOpenConfirm {
        pub port_id: String,
        pub channel_id: ChannelId,
        pub proofs: Proofs,
    }
    #[doc = "`MsgChannelOpenInit(string,((string,string),uint8,uint8,(string,string),string[],string))`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgChannelOpenInit {
        pub port_id: String,
        pub channel: ChannelEnd,
    }
    #[doc = "`MsgChannelOpenTry(string,(string,string),((string,string),uint8,uint8,(string,string),string[],string),string,(uint256,bytes,bytes,bytes,bytes))`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgChannelOpenTry {
        pub port_id: String,
        pub previous_channel_id: ChannelId,
        pub channel: ChannelEnd,
        pub counterparty_version: String,
        pub proofs: Proofs,
    }
    #[doc = "`MsgClientCreate((string,uint8,bytes32,bytes32,uint256,uint256,bytes),(uint256,bytes32,bytes))`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgClientCreate {
        pub client: ClientState,
        pub consensus: ConsensusState,
    }
    #[doc = "`MsgClientMisbehaviour(string,bytes,bytes)`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgClientMisbehaviour {
        pub client_id: String,
        pub header_1_bytes: ethers::core::types::Bytes,
        pub header_2_bytes: ethers::core::types::Bytes,
    }
    #[doc = "`MsgClientUpdate(string,bytes)`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgClientUpdate {
        pub client_id: String,
        pub header_bytes: ethers::core::types::Bytes,
    }
    #[doc = "`MsgConnectionOpenAck(string,string,(string,uint8,bytes32,bytes32,uint256,uint256,bytes),(uint256,bytes,bytes,bytes,bytes),string)`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgConnectionOpenAck {
        pub connection_id: String,
        pub counterparty_connection_id: String,
        pub client_state: ClientState,
        pub proofs: Proofs,
        pub version: String,
    }
    #[doc = "`MsgConnectionOpenConfirm(string,(uint256,bytes,bytes,bytes,bytes))`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgConnectionOpenConfirm {
        pub connection_id: String,
        pub proofs: Proofs,
    }
    #[doc = "`MsgConnectionOpenInit(string,(string,string,bytes),string,uint256)`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgConnectionOpenInit {
        pub client_id: String,
        pub counterparty: ConnectionId,
        pub version: String,
        pub delay_duration: ethers::core::types::U256,
    }
    #[doc = "`MsgConnectionOpenTry(string,string,(string,uint8,bytes32,bytes32,uint256,uint256,bytes),(string,string,bytes),string[],(uint256,bytes,bytes,bytes,bytes),uint256)`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgConnectionOpenTry {
        pub previous_connection_id: String,
        pub client_id: String,
        pub client_state: ClientState,
        pub counterparty: ConnectionId,
        pub counterparty_versions: Vec<String>,
        pub proofs: Proofs,
        pub delay_period: ethers::core::types::U256,
    }
    #[doc = "`MsgRecvPacket((uint256,string,string,string,string,bytes,bytes32,uint256),(uint256,bytes,bytes,bytes,bytes))`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgRecvPacket {
        pub packet: Packet,
        pub proofs: Proofs,
    }
    #[doc = "`MsgTimeoutPacket((uint256,string,string,string,string,bytes,bytes32,uint256),uint256,(uint256,bytes,bytes,bytes,bytes))`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct MsgTimeoutPacket {
        pub packet: Packet,
        pub next_sequence_recv: ethers::core::types::U256,
        pub proofs: Proofs,
    }
    #[doc = "`Packet(uint256,string,string,string,string,bytes,bytes32,uint256)`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct Packet {
        pub sequence: ethers::core::types::U256,
        pub source_port_id: String,
        pub source_channel_id: String,
        pub destination_port_id: String,
        pub destination_channel_id: String,
        pub payload: ethers::core::types::Bytes,
        pub timeout_height: [u8; 32],
        pub timeout_timestamp: ethers::core::types::U256,
    }
    #[doc = "`Proofs(uint256,bytes,bytes,bytes,bytes)`"]
    #[derive(
        Clone,
        Debug,
        Default,
        Eq,
        PartialEq,
        ethers :: contract :: EthAbiType,
        ethers :: contract :: EthAbiCodec,
    )]
    pub struct Proofs {
        pub height: ethers::core::types::U256,
        pub object_proof: ethers::core::types::Bytes,
        pub client_proof: ethers::core::types::Bytes,
        pub consensus_proof: ethers::core::types::Bytes,
        pub other_proof: ethers::core::types::Bytes,
    }
}
