FROM rust:1.70 as builder
WORKDIR /usr/src/tcps
COPY . .
RUN cargo install --path .

FROM gcr.io/distroless/cc-debian10
COPY --from=builder /usr/local/cargo/bin/tcps /usr/local/bin/tcps
CMD ["tcps"]