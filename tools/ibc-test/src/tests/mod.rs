pub mod channel;

#[test]
fn test_config() {
    let path = "config.toml";
    relayer::config::load(path).unwrap();
}
