FROM rust:1-buster as builder

# Install processing-java
RUN wget https://github.com/processing/processing4/releases/download/processing-1286-4.0.1/processing-4.0.1-linux-x64.tgz
RUN tar xvf processing-4.0.1-linux-x64.tgz
RUN mv processing-4.0.1 /usr/local/bin/processing

WORKDIR /app

### Hacky magic to cache cargo dependencies
RUN echo "fn main() {}" > dummy.rs

COPY Cargo.toml .

RUN sed -i 's#src/main.rs#dummy.rs#' Cargo.toml

RUN CARGO_NET_GIT_FETCH_WITH_CLI=true cargo build --release

RUN sed -i 's#dummy.rs#src/main.rs#' Cargo.toml
### End hacky magic

COPY . .

RUN cargo build --release

####

FROM rust:1-buster

COPY --from=builder /app/target/release/test-executor-rs /usr/local/bin/test-executor-rs
COPY --from=builder /usr/local/bin/processing /usr/local/bin/processing

CMD ["./target/release/test-executor-rs"]


