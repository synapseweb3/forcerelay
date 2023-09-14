DESCRIPTION:
Adds key to a configured chain or restores a key to a configured chain using a mnemonic

USAGE:
    forcerelay keys add [OPTIONS] --chain <CHAIN_ID> --key-file <KEY_FILE>

    forcerelay keys add [OPTIONS] --chain <CHAIN_ID> --mnemonic-file <MNEMONIC_FILE>

    forcerelay keys add [OPTIONS] --chain <CHAIN_ID> --secret-file <SECRET_FILE>

OPTIONS:
    -h, --help                   Print help information
        --hd-path <HD_PATH>      Derivation path for this key [default: m/44'/118'/0'/0/0]
        --key-name <KEY_NAME>    Name of the key (defaults to the `key_name` defined in the config)
        --overwrite              Overwrite the key if there is already one with the same key name

FLAGS:
        --chain <CHAIN_ID>
            Identifier of the chain

        --key-file <KEY_FILE>
            Path to the key file

        --mnemonic-file <MNEMONIC_FILE>
            Path to file containing mnemonic to restore the key from

        --secret-file <SECRET_FILE>
            Path to file containing hex-encoded secret key to restore from
