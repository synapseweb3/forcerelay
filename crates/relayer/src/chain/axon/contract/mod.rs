mod generate;
pub use generate::*;

#[cfg(test)]
mod contract_generator {
    use ethers::contract::Abigen;

    #[ignore = "generated file cannot be used until mannully fixing compile errors"]
    #[test]
    fn generate_rs() {
        Abigen::new(
            "OwnableIBCHandler",
            "./src/chain/axon/contract/OwnableIBCHandler.json",
        )
        .unwrap()
        .generate()
        .unwrap()
        .write_to_file("./src/chain/axon/contract/generate.rs")
        .unwrap();
    }
}
