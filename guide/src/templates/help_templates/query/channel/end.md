DESCRIPTION:
Query channel end

USAGE:
    forcerelay query channel end [OPTIONS] --chain <CHAIN_ID> --port <PORT_ID> --channel <CHANNEL_ID>

OPTIONS:
    -h, --help               Print help information
        --height <HEIGHT>    Height of the state to query

REQUIRED:
        --chain <CHAIN_ID>        Identifier of the chain to query
        --channel <CHANNEL_ID>    Identifier of the channel to query [aliases: chan]
        --port <PORT_ID>          Identifier of the port to query
