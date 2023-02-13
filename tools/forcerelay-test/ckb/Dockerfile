# FROM nervos/ckb:latest
FROM nervos/ckb:latest

COPY ./ckb.toml ckb.toml
COPY ./dev.toml dev.toml
COPY ./deploy-contracts.json deploy-contracts.json
COPY ./run.sh run.sh

RUN /bin/sh run.sh
