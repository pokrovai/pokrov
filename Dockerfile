FROM rust:1.94-trixie AS builder

WORKDIR /app
ARG RU_NER_MODEL_ID=r1char9/ner-rubert-tiny-news
ARG SKIP_NER_DOWNLOAD=false
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY crates ./crates
COPY scripts ./scripts
COPY models ./models

RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 python3-pip python3-venv ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN python3 -m venv /opt/ner-venv
RUN /opt/ner-venv/bin/pip install --no-cache-dir --upgrade pip
RUN /opt/ner-venv/bin/pip install --no-cache-dir torch transformers optimum onnx onnxscript

RUN chmod +x ./scripts/download-ner-model.sh
RUN if [ "${SKIP_NER_DOWNLOAD}" != "true" ] && ( [ ! -f models/bert-base-NER/model.onnx ] || [ ! -f models/bert-base-NER/tokenizer.json ] ); then \
      PATH="/opt/ner-venv/bin:${PATH}" ./scripts/download-ner-model.sh dslim/bert-base-NER models/bert-base-NER; \
    fi
RUN if [ "${SKIP_NER_DOWNLOAD}" != "true" ] && ( [ ! -f models/ner-rubert-tiny-news/model.onnx ] || [ ! -f models/ner-rubert-tiny-news/tokenizer.json ] ); then \
      PATH="/opt/ner-venv/bin:${PATH}" ./scripts/download-ner-model.sh "${RU_NER_MODEL_ID}" models/ner-rubert-tiny-news; \
    fi

RUN cargo build --release -p pokrov-runtime --features ner

FROM debian:trixie-slim

RUN useradd --uid 10001 --create-home --shell /usr/sbin/nologin pokrov
WORKDIR /app

COPY --from=builder /app/target/release/pokrov-runtime /usr/local/bin/pokrov-runtime
COPY config/pokrov.example.yaml /app/config/pokrov.yaml
COPY --from=builder /app/models /app/models

USER pokrov
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/pokrov-runtime"]
CMD ["--config", "/app/config/pokrov.yaml"]
