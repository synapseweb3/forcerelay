- Fix an issue where Forcerelay would sometimes fails to retrieve the
  begin/end block events because of a quirk of the Tendermint event indexer
  ([#2867](https://github.com/informalsystems/ibc-rs/issues/2867))