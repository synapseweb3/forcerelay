use core::time::Duration;

use ibc_relayer_types::core::ics04_channel::packet::Packet;

use crate::chain::chain_type::ChainType;
use crate::chain::cli::transfer::local_transfer_token;
use crate::chain::driver::ChainDriver;
use crate::chain::tagged::TaggedChainDriverExt;
use crate::error::Error;
use crate::ibc::token::TaggedTokenRef;
use crate::relayer::transfer::{batched_ibc_token_transfer, ibc_token_transfer};
use crate::types::id::{TaggedChannelIdRef, TaggedPortIdRef};
use crate::types::tagged::*;
use crate::types::wallet::{Wallet, WalletAddress};

pub trait ChainTransferMethodsExt<Chain> {
    /**
       Submits an IBC token transfer transaction to `Chain` to any other
       `Counterparty` chain.

       The following parameters are accepted:

       - A `PortId` on `Chain` that corresponds to a channel connected to
         `Counterparty`.

       - A `ChannelId` on `Chain` that corresponds to a channel connected to
         `Counterparty`.

       - The [`Wallet`] of the sender on `Chain`.

       - The [`WalletAddress`] address of the recipient on `Counterparty`.

       - The denomination of the amount on `Chain`.

       - The transfer amount.
    */
    fn ibc_transfer_token<Counterparty>(
        &self,
        port_id: &TaggedPortIdRef<Chain, Counterparty>,
        channel_id: &TaggedChannelIdRef<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        token: &TaggedTokenRef<Chain>,
    ) -> Result<Packet, Error>;

    fn ibc_transfer_token_with_memo_and_timeout<Counterparty>(
        &self,
        port_id: &TaggedPortIdRef<Chain, Counterparty>,
        channel_id: &TaggedChannelIdRef<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        token: &TaggedTokenRef<Chain>,
        memo: Option<String>,
        timeout: Option<Duration>,
    ) -> Result<Packet, Error>;

    fn ibc_transfer_token_multiple<Counterparty>(
        &self,
        port_id: &TaggedPortIdRef<Chain, Counterparty>,
        channel_id: &TaggedChannelIdRef<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        token: &TaggedTokenRef<Chain>,
        num_msgs: usize,
        memo: Option<String>,
    ) -> Result<(), Error>;

    fn local_transfer_token(
        &self,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Chain, &WalletAddress>,
        token: &TaggedTokenRef<Chain>,
    ) -> Result<(), Error>;
}

impl<'a, Chain: Send> ChainTransferMethodsExt<Chain> for MonoTagged<Chain, &'a ChainDriver> {
    fn ibc_transfer_token<Counterparty>(
        &self,
        port_id: &TaggedPortIdRef<Chain, Counterparty>,
        channel_id: &TaggedChannelIdRef<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        token: &TaggedTokenRef<Chain>,
    ) -> Result<Packet, Error> {
        match self.value().chain_type {
            ChainType::Axon => {
                let websocket_addr = self.value().websocket_address();
                let home_path = self.value().home_path.clone();
                self.value()
                    .runtime
                    .block_on(crate::relayer::axon::transfer::ibc_token_transfer(
                        websocket_addr,
                        home_path,
                        port_id,
                        channel_id,
                        sender,
                        recipient,
                        token,
                        None,
                    ))
            }
            ChainType::Ckb => {
                unimplemented!("Do not support ibc transfer token")
            }
            _ => {
                let rpc_client = self.rpc_client()?;
                self.value().runtime.block_on(ibc_token_transfer(
                    rpc_client.as_ref(),
                    &self.tx_config(),
                    port_id,
                    channel_id,
                    sender,
                    recipient,
                    token,
                    None,
                    None,
                ))
            }
        }
    }

    fn ibc_transfer_token_with_memo_and_timeout<Counterparty>(
        &self,
        port_id: &TaggedPortIdRef<Chain, Counterparty>,
        channel_id: &TaggedChannelIdRef<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        token: &TaggedTokenRef<Chain>,
        memo: Option<String>,
        timeout: Option<Duration>,
    ) -> Result<Packet, Error> {
        match self.value().chain_type {
            ChainType::Axon => {
                let websocket_addr = self.value().websocket_address();
                let home_path = self.value().home_path.clone();
                self.value()
                    .runtime
                    .block_on(crate::relayer::axon::transfer::ibc_token_transfer(
                        websocket_addr,
                        home_path,
                        port_id,
                        channel_id,
                        sender,
                        recipient,
                        token,
                        None,
                    ))
            }
            ChainType::Ckb => {
                unimplemented!("Do not support ibc transfer token")
            }
            _ => {
                let rpc_client = self.rpc_client()?;
                self.value().runtime.block_on(ibc_token_transfer(
                    rpc_client.as_ref(),
                    &self.tx_config(),
                    port_id,
                    channel_id,
                    sender,
                    recipient,
                    token,
                    memo,
                    timeout,
                ))
            }
        }
    }

    fn ibc_transfer_token_multiple<Counterparty>(
        &self,
        port_id: &TaggedPortIdRef<Chain, Counterparty>,
        channel_id: &TaggedChannelIdRef<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        token: &TaggedTokenRef<Chain>,
        num_msgs: usize,
        memo: Option<String>,
    ) -> Result<(), Error> {
        match self.value().chain_type {
            ChainType::Axon => {
                let websocket_addr = self.value().websocket_address();
                let home_path = self.value().home_path.clone();
                self.value().runtime.block_on(async {
                    for _ in 0..num_msgs {
                        crate::relayer::axon::transfer::ibc_token_transfer(
                            websocket_addr.clone(),
                            home_path.clone(),
                            port_id,
                            channel_id,
                            sender,
                            recipient,
                            token,
                            None,
                        )
                        .await?;
                    }
                    Ok(())
                })
            }
            ChainType::Ckb => {
                unimplemented!("Do not support ibc transfer token")
            }
            _ => {
                let rpc_client = self.rpc_client()?;
                self.value().runtime.block_on(batched_ibc_token_transfer(
                    rpc_client.as_ref(),
                    &self.tx_config(),
                    port_id,
                    channel_id,
                    sender,
                    recipient,
                    token,
                    num_msgs,
                    memo,
                ))
            }
        }
    }

    fn local_transfer_token(
        &self,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Chain, &WalletAddress>,
        token: &TaggedTokenRef<Chain>,
    ) -> Result<(), Error> {
        let driver = *self.value();
        match driver.chain_type {
            ChainType::Axon | ChainType::Ckb => {
                unimplemented!(
                    "chain_type {:?} not support local_transfer_token",
                    driver.chain_type
                )
            }
            _ => local_transfer_token(
                driver.chain_id.as_str(),
                &driver.command_path,
                &driver.home_path,
                &driver.rpc_listen_address(),
                sender.value().address.as_str(),
                recipient.value().as_str(),
                &token.value().to_string(),
            ),
        }
    }
}
