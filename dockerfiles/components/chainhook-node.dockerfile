FROM rust:bullseye AS build

WORKDIR /src

RUN apt update && apt install -y ca-certificates pkg-config libssl-dev libclang-dev
RUN rustup default stable && rustup update

COPY ./Cargo.* /src/
COPY ./components/chainhook-cli /src/components/chainhook-cli
COPY ./components/chainhook-types-rs /src/components/chainhook-types-rs
COPY ./components/chainhook-sdk /src/components/chainhook-sdk

WORKDIR /src/components/chainhook-cli

RUN mkdir /out
RUN cargo build --features release --release
RUN cp /src/target/release/chainhook /out

FROM debian:bullseye-slim

RUN apt update && apt install -y ca-certificates libssl-dev

COPY --from=build /out/ /bin/

WORKDIR /workspace

ENTRYPOINT ["chainhook"]
