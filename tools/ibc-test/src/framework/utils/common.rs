use futures::Future;
use secp256k1::{
    rand::{self, Rng},
    SecretKey,
};
use tokio::runtime::Runtime;

fn get_rt() -> &'static Runtime {
    lazy_static::lazy_static! {
        static ref RT: Runtime = Runtime::new().unwrap();
    }
    &RT
}

pub fn wait_task<F: Future>(f: F) -> F::Output {
    get_rt().block_on(f)
}

pub fn gen_secp256k1_private_key() -> SecretKey {
    let mut rng = rand::thread_rng();
    let mut private_key = [0u8; 32];
    rng.fill(&mut private_key);
    SecretKey::from_slice(&private_key).unwrap()
}
