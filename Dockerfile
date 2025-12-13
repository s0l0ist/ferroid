# ---- Builder stage (installs tools + builds) ----
FROM public.ecr.aws/docker/library/alpine:latest AS builder

# Install build-time dependencies
RUN apk update && apk add --no-cache \
    ca-certificates \
    curl \
    file \
    g++ \
    gcc

ENV PROTOC_VERSION=33.2

ARG TARGETARCH

# Download correct protoc binary based on arch
RUN case "$TARGETARCH" in \
    amd64) PROTOC_ZIP="protoc-${PROTOC_VERSION}-linux-x86_64.zip" ;; \
    arm64) PROTOC_ZIP="protoc-${PROTOC_VERSION}-linux-aarch_64.zip" ;; \
    *) echo "Unsupported arch: $TARGETARCH" && exit 1 ;; \
    esac && \
    curl -LO "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/${PROTOC_ZIP}" && \
    unzip "${PROTOC_ZIP}" -d /usr/local && \
    rm "${PROTOC_ZIP}"

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /work

COPY . .

RUN cargo build --bin ferroid-tonic-server --profile bin-release --features tracing,metrics,honeycomb

# ---- Final minimal stage ----
FROM scratch AS app

WORKDIR /app

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /work/target/bin-release/ferroid-tonic-server /app/

EXPOSE 50051
CMD ["/app/ferroid-tonic-server"]
