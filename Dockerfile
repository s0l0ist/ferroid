# ---- Builder stage (installs tools + builds) ----
FROM public.ecr.aws/docker/library/alpine:latest AS builder

# Install build-time dependencies
RUN apk update && apk add --no-cache \
    curl \
    file \
    g++ \
    gcc

ENV PROTOC_VERSION=31.1

# Download and install protoc
RUN curl -LO https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-linux-aarch_64.zip && \
    unzip protoc-${PROTOC_VERSION}-linux-aarch_64.zip -d /usr/local && \
    rm protoc-${PROTOC_VERSION}-linux-aarch_64.zip

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /work

COPY . .

RUN cargo build --bin tonic-server --profile bin-release --features tracing,metrics

# ---- Final minimal stage ----
FROM scratch AS app

WORKDIR /app
COPY --from=builder /work/target/bin-release/tonic-server /app/

EXPOSE 50051
CMD ["/app/tonic-server"]
