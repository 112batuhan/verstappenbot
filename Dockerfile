FROM rust:latest as rust-builder
ARG DATABASE_URL
COPY vosk/lib/libvosk.so usr/lib/libvosk.so
COPY vosk/lib/vosk_api.h usr/lib/vosk_api.h
WORKDIR /usr/src/verstappenbot
COPY . .
RUN apt update
RUN apt install -y libopus-dev
ENV DATABASE_URL=${DATABASE_URL}
RUN cargo build --release

FROM rust:slim
ARG DISCORD_TOKEN
ARG DATABASE_URL
RUN apt update
RUN apt install -y libopus-dev
RUN rm -rf /var/lib/apt/lists/*
COPY vosk/lib/libvosk.so usr/lib/libvosk.so
COPY vosk/lib/vosk_api.h usr/lib/vosk_api.h
WORKDIR /usr/src/verstappenbot
COPY --from=rust-builder /usr/src/verstappenbot/target/release/verstappenbot .
COPY vosk/model vosk/model
RUN mkdir -p /songs
ENV DISCORD_TOKEN=${DISCORD_TOKEN}
ENV DATABASE_URL=${DATABASE_URL}
ENTRYPOINT ["./verstappenbot"]
