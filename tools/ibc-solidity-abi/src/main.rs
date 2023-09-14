use ethers::prelude::{MultiAbigen, SelectContracts};
use eyre::Result;
use std::path::{Path, PathBuf};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    log::info!("Generate Rust abi");
    let abi_source: PathBuf = "./tools/ibc-solidity-abi/src/abis".into();
    let output_dir: PathBuf = "./tools/ibc-solidity-abi/src/generated".into();
    rust_file_generation(&abi_source, &output_dir)?;
    log::info!("Generated abi at {output_dir:?}");
    Ok(())
}

fn rust_file_generation<P: AsRef<Path>>(abi_dir: P, out_dir: P) -> Result<()> {
    let gen = MultiAbigen::from_json_files(abi_dir)
        .unwrap()
        .with_filter(SelectContracts::default().add_pattern("MockTransfer"));
    gen.build()?.write_to_module(out_dir, true)
}
