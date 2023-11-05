# Build stage
FROM rust:bookworm as build

RUN USER=root cargo new --bin deezer
WORKDIR /deezer

COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# cache deps
RUN cargo build --release

RUN rm ./src/*.rs
COPY ./src ./src

# copy sql migration files
COPY ./migrations ./migrations

RUN rm ./target/release/deps/deezer*
RUN cargo build --release

# final runnable image
FROM debian:bookworm-slim

RUN apt update && apt upgrade -y && apt install -y libssl-dev ca-certificates
COPY --from=build /deezer/target/release/deezer .


RUN chmod a+x ./deezer
CMD ["./deezer"]