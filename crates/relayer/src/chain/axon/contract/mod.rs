mod contract;
pub use contract::*;

#[cfg(test)]
mod contract_generator {
    use ethers::contract::Abigen;

    #[test]
    fn generate_rs() {
        Abigen::new(
            "OwnableIBCHandler",
            "./src/chain/axon/contract/OwnableIBCHandler.json",
        )
        .unwrap()
        .generate()
        .unwrap()
        .write_to_file("./src/chain/axon/contract/contract.rs")
        .unwrap();
    }
}
