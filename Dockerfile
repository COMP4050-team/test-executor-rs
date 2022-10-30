FROM rust:1-buster as builder

# Install processing-java
RUN wget https://github.com/processing/processing4/releases/download/processing-1286-4.0.1/processing-4.0.1-linux-x64.tgz
RUN tar xvf processing-4.0.1-linux-x64.tgz
RUN mv processing-4.0.1 /usr/local/bin/processing

WORKDIR /app

# Add muscl-gcc so we can statically link libc.
# The runner image is alpine and doesn't have the right version of glibc available for dynamic linking
RUN apt update
RUN apt install -y musl-tools

RUN rustup target add x86_64-unknown-linux-musl

### Hacky magic to cache cargo dependencies
RUN echo "fn main() {}" > dummy.rs

COPY Cargo.toml .

RUN sed -i 's#src/main.rs#dummy.rs#' Cargo.toml

RUN CARGO_NET_GIT_FETCH_WITH_CLI=true cargo build --release --target x86_64-unknown-linux-musl

RUN sed -i 's#dummy.rs#src/main.rs#' Cargo.toml
### End hacky magic

COPY . .

RUN cargo build --release --target x86_64-unknown-linux-musl

####

FROM amazoncorretto:8

WORKDIR /app

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/test-executor-rs test-executor-rs
COPY --from=builder /usr/local/bin/processing /usr/local/bin/processing

COPY templates ./templates

COPY docker-entrypoint.sh ./

ENTRYPOINT [ "./docker-entrypoint.sh" ]
