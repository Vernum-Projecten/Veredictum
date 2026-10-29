# syntax=docker/dockerfile:1
# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
#
# A stand-in for the release image, for the GitHub Action's CI job (#770).
#
# A pull request has no release image to run, and building deploy/docker/Dockerfile
# from source costs the console's whole WASM build. This image carries what
# the action reads from the release image and nothing else: the engine at the
# release image's path as the entry point, the Arazzo catalogue and the
# vendored specification trees under /work, VEREDICTUM_CATALOGUE pointing at
# the catalogue, and the same digest-pinned non-root base. The release image's
# entry point is the console, which hands `test` and `export` to that same
# engine; the release pipeline's smoke run proves that hand-off.
#
# The `stub-sut` target is the job's fake server under test, the crate's
# `stub_sut` example, in its own image on the same base.
#
# Built from the repository root by scripts/action/stand-in-image.sh. The
# builder is the release image's own digest-pinned builder base, and both
# binaries are debug builds: the images are judged on what they wire, and a
# debug build compiles in less time.
FROM rust:1.98.1-slim-trixie@sha256:4cd829461bd5c4d511c32e269da9cb8929223b666519d8004e35fc8d1d771ab7 AS builder
WORKDIR /app
COPY . .
RUN --mount=type=cache,target=/app/target,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo build --locked -p veredictum --bin veredictum --example stub_sut \
    && install -m 0755 target/debug/veredictum /usr/local/bin/veredictum \
    && install -m 0755 target/debug/examples/stub_sut /usr/local/bin/stub_sut

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:3ee00291cf386ec52a552e6a858d555debd74776eb4bfa53fa3df2cd6677515e AS stub-sut
USER 65532:65532
COPY --from=builder /usr/local/bin/stub_sut /usr/local/bin/stub_sut
ENTRYPOINT ["/usr/local/bin/stub_sut"]

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:3ee00291cf386ec52a552e6a858d555debd74776eb4bfa53fa3df2cd6677515e AS engine
USER 65532:65532
WORKDIR /app
ENV VEREDICTUM_CATALOGUE=/work/catalogue
COPY --from=builder /usr/local/bin/veredictum /usr/local/bin/veredictum
# The catalogue names its OAS sources as `../../../specs/rest-oas/…`, so it
# sits one level below /work beside specs/, as it does in the repository.
COPY catalogue/ /work/catalogue/
COPY specs/openehr/ /work/specs/openehr/
COPY specs/its-xml-schemas/ /work/specs/its-xml-schemas/
COPY specs/its-json-schemas/ /work/specs/its-json-schemas/
COPY specs/rest-oas/ /work/specs/rest-oas/
ENTRYPOINT ["/usr/local/bin/veredictum"]
