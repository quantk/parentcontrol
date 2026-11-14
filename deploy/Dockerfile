# parentcontrol-server: a static binary in an otherwise empty image.
# Build from the repository root: docker build -f deploy/Dockerfile .

FROM rust:1.99-alpine AS build
# ring compiles C code; musl-dev provides the static C runtime.
RUN apk add --no-cache musl-dev gcc
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p parentcontrol-server \
 && mkdir -p /out/data \
 && cp target/release/parentcontrol-server /out/ \
 && chown 10001:10001 /out/data

FROM scratch
COPY --from=build /out/parentcontrol-server /parentcontrol-server
# A fresh named volume takes the owner of this directory, so the
# unprivileged server can write to it.
COPY --from=build --chown=10001:10001 /out/data /data
USER 10001:10001
ENV PARENTCONTROL_DATA_DIR=/data \
    PARENTCONTROL_LISTEN=0.0.0.0:8443
EXPOSE 8443
VOLUME ["/data"]
ENTRYPOINT ["/parentcontrol-server"]
CMD ["serve"]
