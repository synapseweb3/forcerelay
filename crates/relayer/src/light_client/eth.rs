mod utils;

use std::cmp;
use std::ops::Index;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use eyre::eyre;
use eyre::Result;
use ibc_relayer_types::clients::ics07_eth::client_state::ClientState as EthClientState;
use ibc_relayer_types::clients::ics07_eth::types::{
    BitVector, Bootstrap, ConsensusError, FinalityUpdate, GenericUpdate, PublicKey, SignatureBytes,
    SyncCommittee, TreeHash, Update, H256, U512,
};
use ibc_relayer_types::core::ics02_client::client_state::ClientState;
use ibc_relayer_types::core::ics02_client::client_type::ClientType;
use ibc_relayer_types::core::ics02_client::error::Error as ClientError;
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use ibc_relayer_types::{
    clients::ics07_eth::header::Header, core::ics02_client::events::UpdateClient, Height,
};
use reqwest_middleware::ClientBuilder;
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;

use crate::config::eth::EthChainConfig;
use crate::{
    chain::{endpoint::ChainEndpoint, eth::EthChain},
    client_state::AnyClientState,
    config::ChainConfig,
    error::Error,
    misbehaviour::MisbehaviourEvidence,
};

use super::Verified;

use self::utils::calc_sync_period;
use self::utils::compute_domain;
use self::utils::compute_signing_root;
use self::utils::is_aggregate_valid;
use self::utils::is_current_committee_proof_valid;
use self::utils::is_finality_proof_valid;
use self::utils::is_next_committee_proof_valid;

pub const MAX_REQUEST_LIGHT_CLIENT_UPDATES: u8 = 128;

pub struct ConsensusClient<R: ConsensusRpc> {
    pub rpc: R,
    pub store: LightClientStore,
    pub initial_checkpoint: [u8; 32],      // Vec<u8>
    pub last_checkpoint: Option<[u8; 32]>, // Vec<u8>
    pub config: Arc<EthChainConfig>,
}

impl<R: ConsensusRpc> ConsensusClient<R> {
    pub fn new(
        rpc: &str,
        checkpoint_block_root: &[u8],
        config: Arc<EthChainConfig>,
    ) -> Result<ConsensusClient<R>> {
        let rpc = R::new(rpc);
        let initial_checkpoint = checkpoint_block_root.to_vec().try_into().unwrap(); // FIXME
        Ok(ConsensusClient {
            rpc,
            store: LightClientStore::default(),
            initial_checkpoint,
            last_checkpoint: None,
            config,
        })
    }

    pub async fn bootstrap(&mut self) -> Result<()> {
        let mut bootstrap = self
            .rpc
            .get_bootstrap(&self.initial_checkpoint)
            .await
            .map_err(|e| eyre!("could not fetch bootstrap: {}", e))?;

        let is_valid = self.is_valid_checkpoint(bootstrap.header.slot);
        if !is_valid {
            return Err(ConsensusError::CheckpointTooOld.into());
        }

        let header_hash = bootstrap.header.tree_hash_root();
        let expected_hash = H256::from(&self.initial_checkpoint);

        if header_hash != expected_hash {
            return Err(ConsensusError::InvalidHeaderHash(expected_hash, header_hash).into());
        }

        let committee_valid = is_current_committee_proof_valid(
            &bootstrap.header,
            &mut bootstrap.current_sync_committee,
            &mut bootstrap.current_sync_committee_branch,
        );

        if !committee_valid {
            return Err(ConsensusError::InvalidCurrentSyncCommitteeProof.into());
        }

        self.store = LightClientStore {
            finalized_header: bootstrap.header.clone(),
            current_sync_committee: bootstrap.current_sync_committee,
            next_sync_committee: None,
            previous_max_active_participants: 0,
            current_max_active_participants: 0,
        };

        Ok(())
    }

    pub async fn sync(&mut self) -> Result<()> {
        self.bootstrap().await?;

        let current_period = calc_sync_period(self.store.finalized_header.slot);
        let updates = self
            .rpc
            .get_updates(current_period, MAX_REQUEST_LIGHT_CLIENT_UPDATES)
            .await?;
        for update in updates {
            self.verify_update(&update)?;
            self.apply_update(&update);
        }

        let finality_update = self.rpc.get_finality_update().await?;
        self.verify_finality_update(&finality_update)?;
        self.apply_finality_update(&finality_update)?;

        Ok(())
    }

    fn apply_update(&mut self, update: &Update) {
        let update = GenericUpdate::from(update);
        self.apply_generic_update(&update);
    }

    pub fn apply_finality_update(&mut self, update: &FinalityUpdate) -> Result<()> {
        let update = GenericUpdate::from(update);
        self.apply_generic_update(&update);
        Ok(())
    }

    pub fn apply_generic_update(&mut self, update: &GenericUpdate) {
        let committee_bits = &update.sync_aggregate.sync_committee_bits.num_set_bits();

        self.store.current_max_active_participants = u64::max(
            self.store.current_max_active_participants,
            *committee_bits as u64,
        );

        let update_attested_period = calc_sync_period(update.attested_header.slot);

        let update_finalized_slot = update
            .finalized_header
            .as_ref()
            .map(|h| h.slot)
            .unwrap_or(0);

        let update_finalized_period = calc_sync_period(update_finalized_slot);

        let update_has_finalized_next_committee = self.store.next_sync_committee.is_none()
            && self.has_sync_update(update)
            && self.has_finality_update(update)
            && update_finalized_period == update_attested_period;

        let should_apply_update = {
            let has_majority = committee_bits * 3 >= 512 * 2;
            let update_is_newer = update_finalized_slot > self.store.finalized_header.slot;
            let good_update = update_is_newer || update_has_finalized_next_committee;

            has_majority && good_update
        };

        if should_apply_update {
            let store_period = calc_sync_period(self.store.finalized_header.slot);

            if self.store.next_sync_committee.is_none() {
                self.store.next_sync_committee = update.next_sync_committee.clone();
            } else if update_finalized_period == store_period + 1 {
                self.store.current_sync_committee = self.store.next_sync_committee.clone().unwrap();
                self.store.next_sync_committee = update.next_sync_committee.clone();
                self.store.previous_max_active_participants =
                    self.store.current_max_active_participants;
                self.store.current_max_active_participants = 0;
            }

            if update_finalized_slot > self.store.finalized_header.slot {
                self.store.finalized_header = update.finalized_header.clone().unwrap();
                // self.log_finality_update(update);

                if self.store.finalized_header.slot % 32 == 0 {
                    let checkpoint_res = self.store.finalized_header.tree_hash_root();
                    self.last_checkpoint = Some(checkpoint_res.into())
                }
            }
        }
    }

    fn has_sync_update(&self, update: &GenericUpdate) -> bool {
        update.finalized_header.is_some() && update.finality_branch.is_some()
    }

    fn has_finality_update(&self, update: &GenericUpdate) -> bool {
        update.finalized_header.is_some() && update.finality_branch.is_some()
    }

    pub fn verify_finality_update(&self, update: &FinalityUpdate) -> Result<()> {
        let update = GenericUpdate::from(update);
        self.verify_generic_update(&update)
    }

    fn verify_update(&self, update: &Update) -> Result<()> {
        let update = GenericUpdate::from(update);
        self.verify_generic_update(&update)
    }

    fn verify_generic_update(&self, update: &GenericUpdate) -> Result<()> {
        let bits = &update.sync_aggregate.sync_committee_bits.num_set_bits();
        if *bits == 0 {
            return Err(ConsensusError::InsufficientParticipation.into());
        }

        let update_finalized_slot = update.finalized_header.clone().unwrap_or_default().slot;
        let valid_time = self.expected_current_slot() >= update.signature_slot
            && update.signature_slot > update.attested_header.slot
            && update.attested_header.slot >= update_finalized_slot;

        if !valid_time {
            return Err(ConsensusError::InvalidTimestamp.into());
        }

        let store_period = calc_sync_period(self.store.finalized_header.slot);
        let update_sig_period = calc_sync_period(update.signature_slot);
        let valid_period = if self.store.next_sync_committee.is_some() {
            update_sig_period == store_period || update_sig_period == store_period + 1
        } else {
            update_sig_period == store_period
        };

        if !valid_period {
            return Err(ConsensusError::InvalidPeriod.into());
        }

        let update_attested_period = calc_sync_period(update.attested_header.slot);
        let update_has_next_committee = self.store.next_sync_committee.is_none()
            && update.next_sync_committee.is_some()
            && update_attested_period == store_period;

        if update.attested_header.slot <= self.store.finalized_header.slot
            && !update_has_next_committee
        {
            return Err(ConsensusError::NotRelevant.into());
        }

        if update.attested_header.slot <= self.store.finalized_header.slot
            && !update_has_next_committee
        {
            return Err(ConsensusError::NotRelevant.into());
        }

        if update.finalized_header.is_some() && update.finality_branch.is_some() {
            let is_valid = is_finality_proof_valid(
                &update.attested_header,
                &mut update.finalized_header.clone().unwrap(),
                &update.finality_branch.clone().unwrap(),
            );

            if !is_valid {
                return Err(ConsensusError::InvalidFinalityProof.into());
            }
        }

        if update.next_sync_committee.is_some() && update.finality_branch.is_some() {
            let is_valid = is_next_committee_proof_valid(
                &update.attested_header,
                &mut update.next_sync_committee.clone().unwrap(),
                &update.next_sync_committee_branch.clone().unwrap(),
            );

            if !is_valid {
                return Err(ConsensusError::InvalidNextSyncCommitteeProof.into());
            }
        }

        let sync_committee = if update_sig_period == store_period {
            &self.store.current_sync_committee
        } else {
            self.store.next_sync_committee.as_ref().unwrap()
        };
        let pks =
            get_participating_keys(sync_committee, &update.sync_aggregate.sync_committee_bits)?;

        let is_valid_sig = self.verify_sync_committee_signature(
            &pks,
            &update.attested_header,
            &update.sync_aggregate.sync_committee_signature,
            update.signature_slot,
        );

        if !is_valid_sig {
            return Err(ConsensusError::InvalidSignature.into());
        }

        Ok(())
    }

    fn verify_sync_committee_signature(
        &self,
        pks: &[PublicKey],
        attested_header: &Header,
        signature: &SignatureBytes,
        signature_slot: u64,
    ) -> bool {
        let res: Result<bool> = (move || {
            let pks: Vec<&PublicKey> = pks.iter().collect();
            let header_root = attested_header.tree_hash_root();
            let signing_root = self.compute_committee_sign_root(header_root, signature_slot)?;
            Ok(is_aggregate_valid(signature, signing_root, &pks))
        })();

        match res {
            Ok(b) => b,
            Err(_) => false,
        }
    }

    fn compute_committee_sign_root(&self, header: H256, slot: u64) -> Result<H256> {
        let genesis_root = &self.config.genesis_root;
        let domain_type = &hex::decode("07000000")?[..];
        let fork_version = self.config.fork_version(slot);
        let domain = compute_domain(domain_type, fork_version, *genesis_root);
        Ok(compute_signing_root(header, domain))
    }

    fn is_valid_checkpoint(&self, blockhash_slot: u64) -> bool {
        let current_slot = self.expected_current_slot();
        let current_slot_timestamp = self.slot_timestamp(current_slot);
        let blockhash_slot_timestamp = self.slot_timestamp(blockhash_slot);

        let slot_age = current_slot_timestamp - blockhash_slot_timestamp;

        slot_age < self.config.max_checkpoint_age
    }

    fn slot_timestamp(&self, slot: u64) -> u64 {
        slot * 12 + self.config.genesis_time
    }

    pub fn expected_current_slot(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap();

        let genesis_time = self.config.genesis_time;
        let since_genesis = now - std::time::Duration::from_secs(genesis_time);

        since_genesis.as_secs() / 12
    }
}

#[async_trait]
pub trait ConsensusRpc {
    fn new(path: &str) -> Self;
    async fn get_bootstrap(&self, block_root: &[u8]) -> Result<Bootstrap>;
    async fn get_updates(&self, period: u64, count: u8) -> Result<Vec<Update>>;
    async fn get_finality_update(&self) -> Result<FinalityUpdate>;
}

#[derive(Default)]
pub struct LightClientStore {
    pub finalized_header: Header,
    pub current_sync_committee: SyncCommittee,
    pub next_sync_committee: Option<SyncCommittee>,
    pub previous_max_active_participants: u64,
    pub current_max_active_participants: u64,
}

pub struct NimbusRpc {
    pub rpc: String,
    pub client: ClientWithMiddleware,
}

#[async_trait]
impl ConsensusRpc for NimbusRpc {
    fn new(rpc: &str) -> Self {
        let retry_policy = ExponentialBackoff::builder()
            .backoff_exponent(1)
            .build_with_max_retries(3);
        let client = ClientBuilder::new(reqwest::Client::new())
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();
        NimbusRpc {
            rpc: rpc.to_string(),
            client,
        }
    }

    async fn get_updates(&self, period: u64, count: u8) -> Result<Vec<Update>> {
        let count = cmp::min(count, MAX_REQUEST_LIGHT_CLIENT_UPDATES);
        let req = format!(
            "{}/eth/v1/beacon/light_client/updates?start_period={}&count={}",
            self.rpc, period, count
        );

        let res = self
            .client
            .get(req)
            .send()
            .await?
            .json::<UpdateResponse>()
            .await?;

        Ok(res.iter().map(|d| d.data.clone()).collect())
    }

    async fn get_finality_update(&self) -> Result<FinalityUpdate> {
        let req = format!("{}/eth/v1/beacon/light_client/finality_update", self.rpc);
        let res = self
            .client
            .get(req)
            .send()
            .await?
            .json::<FinalityUpdateResponse>()
            .await?;

        Ok(res.data)
    }

    async fn get_bootstrap(&self, block_root: &[u8]) -> Result<Bootstrap> {
        let root_hex = hex::encode(block_root);
        let req = format!(
            "{}/eth/v1/beacon/light_client/bootstrap/0x{}",
            self.rpc, root_hex
        );

        let res = self
            .client
            .get(req)
            .send()
            .await?
            .json::<BootstrapResponse>()
            .await?;

        Ok(res.data)
    }
}

pub struct LightClient {
    pub chain_id: ChainId,
    pub consensus_client: ConsensusClient<NimbusRpc>,
}

impl LightClient {
    pub fn from_config(config: ChainConfig) -> Result<Self, Error> {
        let eth_config: EthChainConfig = config.try_into()?;
        let light_client = LightClient {
            chain_id: eth_config.id.clone(),
            consensus_client: ConsensusClient::<NimbusRpc> {
                rpc: NimbusRpc::new(&eth_config.rpc_addr.as_ref().unwrap().clone()),
                store: Default::default(),
                initial_checkpoint: eth_config.initial_checkpoint,
                last_checkpoint: None,
                config: Arc::new(eth_config), // should fill real world data
            },
        };
        Ok(light_client)
    }
}

impl super::LightClient<EthChain> for LightClient {
    fn header_and_minimal_set(
        &mut self,
        _trusted: Height,
        _target: Height,
        _client_state: &AnyClientState,
    ) -> Result<Verified<Header>, Error> {
        todo!()
    }

    fn verify(
        &mut self,
        _trusted: Height,
        _target: Height,
        client_state: &AnyClientState,
    ) -> Result<Verified<ChainId>, Error> {
        let eth_client_state: &EthClientState = client_state.try_into().map_err(|_| {
            Error::client_type_mismatch(ClientType::Eth, client_state.client_type())
        })?;
        self.consensus_client
            .verify_update(&eth_client_state.lightclient_update)
            .map_err(|e| {
                Error::light_client_state(ClientError::header_verification_failure(e.to_string()))
            })?;
        Ok(Verified {
            target: client_state.chain_id(),
            supporting: vec![],
        })
    }

    fn check_misbehaviour(
        &mut self,
        _update: &UpdateClient,
        _client_state: &AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error> {
        todo!()
    }

    fn fetch(&mut self, _height: Height) -> Result<<EthChain as ChainEndpoint>::LightBlock, Error> {
        todo!()
    }
}

fn get_participating_keys(
    committee: &SyncCommittee,
    bitfield: &BitVector<U512>,
) -> Result<Vec<PublicKey>> {
    let mut pks: Vec<PublicKey> = Vec::new();
    bitfield.iter().enumerate().for_each(|(i, bit)| {
        if bit {
            let pk = committee.pubkeys.index(i);
            let pk = PublicKey::deserialize(pk).unwrap();
            pks.push(pk.clone());
        }
    });
    Ok(pks)
}

#[derive(serde::Deserialize, Debug)]
struct BootstrapResponse {
    data: Bootstrap,
}

#[derive(serde::Deserialize, Debug)]
struct FinalityUpdateResponse {
    data: FinalityUpdate,
}

type UpdateResponse = Vec<UpdateData>;

#[derive(serde::Deserialize, Debug)]
struct UpdateData {
    data: Update,
}

#[cfg(test)]
mod tests {
    use std::fs::read_to_string;
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::config::eth::EthChainConfig;
    use crate::light_client::eth::utils::calc_sync_period;
    use crate::light_client::eth::MAX_REQUEST_LIGHT_CLIENT_UPDATES;

    use super::Bootstrap;
    use super::ConsensusClient;
    use super::ConsensusRpc;
    use super::FinalityUpdate;
    use super::Result;
    use super::Update;

    use async_trait::async_trait;
    use ibc_relayer_types::clients::ics07_eth::header::Header;
    use ibc_relayer_types::clients::ics07_eth::types::ConsensusError;
    use ibc_relayer_types::clients::ics07_eth::types::FixedVector;

    pub struct MockRpc {
        testdata: PathBuf,
    }

    #[async_trait]
    impl ConsensusRpc for MockRpc {
        fn new(path: &str) -> Self {
            MockRpc {
                testdata: PathBuf::from(path),
            }
        }

        async fn get_bootstrap(&self, _block_root: &[u8]) -> Result<Bootstrap> {
            let bootstrap = read_to_string(self.testdata.join("bootstrap.json"))?;
            Ok(serde_json::from_str(&bootstrap)?)
        }

        async fn get_updates(&self, _period: u64, _count: u8) -> Result<Vec<Update>> {
            let updates = read_to_string(self.testdata.join("updates.json"))?;
            Ok(serde_json::from_str(&updates)?)
        }

        async fn get_finality_update(&self) -> Result<FinalityUpdate> {
            let finality = read_to_string(self.testdata.join("finality.json"))?;
            Ok(serde_json::from_str(&finality)?)
        }
    }

    async fn get_client(large_checkpoint_age: bool) -> ConsensusClient<MockRpc> {
        let base_config = EthChainConfig::goerli();
        let config = EthChainConfig {
            id: base_config.id,
            genesis_time: base_config.genesis_time,
            genesis_root: base_config.genesis_root,
            forks: base_config.forks,
            max_checkpoint_age: if large_checkpoint_age { 123123123 } else { 123 },
            websocket_addr: base_config.websocket_addr,
            rpc_addr: Default::default(),
            rpc_port: Default::default(),
            initial_checkpoint: Default::default(),
            key_name: Default::default(),
        };
        let checkpoint =
            hex::decode("1e591af1e90f2db918b2a132991c7c2ee9a4ab26da496bd6e71e4f0bd65ea870")
                .unwrap();

        let mut client =
            ConsensusClient::new("src/testdata/", &checkpoint, Arc::new(config)).unwrap();
        client.bootstrap().await.unwrap();
        client
    }

    #[tokio::test]
    async fn test_verify_update() {
        let client = get_client(true).await;
        let period = calc_sync_period(client.store.finalized_header.slot);
        let updates = client
            .rpc
            .get_updates(period, MAX_REQUEST_LIGHT_CLIENT_UPDATES)
            .await
            .unwrap();
        let update = updates[0].clone();
        client.verify_update(&update).unwrap();
    }

    #[tokio::test]
    async fn test_verify_finality() {
        let mut client = get_client(true).await;
        client.sync().await.unwrap();

        let update = client.rpc.get_finality_update().await.unwrap();

        client.verify_finality_update(&update).unwrap();
    }

    #[tokio::test]
    async fn test_verify_finality_invalid_finality() {
        let mut client = get_client(true).await;
        client.sync().await.unwrap();

        let mut update = client.rpc.get_finality_update().await.unwrap();
        update.finalized_header = Header::default();

        let err = client.verify_finality_update(&update).err().unwrap();
        assert_eq!(
            err.to_string(),
            ConsensusError::InvalidFinalityProof.to_string()
        );
    }

    #[tokio::test]
    async fn test_verify_update_invalid_sig() {
        let client = get_client(true).await;
        let period = calc_sync_period(client.store.finalized_header.slot);
        let updates = client
            .rpc
            .get_updates(period, MAX_REQUEST_LIGHT_CLIENT_UPDATES)
            .await
            .unwrap();

        let mut update = updates[0].clone();
        update.sync_aggregate.sync_committee_signature = FixedVector::default();

        let err = client.verify_update(&mut update).err().unwrap();
        assert_eq!(
            err.to_string(),
            ConsensusError::InvalidSignature.to_string()
        );
    }

    #[tokio::test]
    async fn test_verify_update_invalid_committee() {
        let client = get_client(true).await;
        let period = calc_sync_period(client.store.finalized_header.slot);
        let updates = client
            .rpc
            .get_updates(period, MAX_REQUEST_LIGHT_CLIENT_UPDATES)
            .await
            .unwrap();

        let mut update = updates[0].clone();
        update.next_sync_committee.pubkeys[0] = FixedVector::default();

        let err = client.verify_update(&update).err().unwrap();
        assert_eq!(
            err.to_string(),
            ConsensusError::InvalidNextSyncCommitteeProof.to_string()
        );
    }
}
