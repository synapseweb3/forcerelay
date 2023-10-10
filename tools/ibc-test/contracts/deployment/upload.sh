#!/usr/bin/env bash

echo "deploying '$1' contract with address '$3' through RPC $2"

ckb-cli --url $2 deploy gen-txs --from-address $3 --fee-rate 1000 --deployment-config ./$1.deployment.toml \
    --info-file ./tx/$1.json --migration-dir ./migration --sign-now

echo "ckb transacion file '$1.json' has generated in ./tx directory"

ckb-cli --url $2 deploy apply-txs --info-file ./tx/$1.json --migration-dir ./migration

echo "deployment finished"

# usage: upload.sh connection https://testnet.ckbapp.dev/ ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsq28phxutezqvjgfv5q38gn5kwek4m9km3cmajeqs