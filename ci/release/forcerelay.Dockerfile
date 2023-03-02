#
# Used for running hermes in docker containers
#
# Usage:
#   docker build . --build-arg TAG=v0.3.0 -t informalsystems/hermes:0.3.0 -f hermes.Dockerfile

FROM rust:1-buster AS build-env

ARG TAG
WORKDIR /root

RUN git clone -b ${TAG} --depth 1 https://github.com/synapseweb3/relayer \
 && cd relayer \
 && cargo build --release

FROM ubuntu:rolling
LABEL maintainer="synapseweb3"
RUN useradd -m forcerelay -s /bin/bash
WORKDIR /home/forcerelay
USER forcerelay:forcerelay
ENTRYPOINT ["/usr/bin/forcerelay"]

COPY --chown=0:0 --from=build-env /root/relayer/target/release/forcerelay /usr/bin/forcerelay
