ARG DISCORD_TOKEN

FROM rust:latest as rust-builder
COPY vosk/lib/libvosk.so usr/lib/libvosk.so
COPY vosk/lib/vosk_api.h usr/lib/vosk_api.h
WORKDIR /usr/src/verstappenbot
COPY . .
RUN apt update
RUN apt install -y libopus-dev
RUN cargo build --release

FROM rust:slim
RUN apt update
RUN apt install -y libopus-dev
RUN rm -rf /var/lib/apt/lists/*
COPY vosk/lib/libvosk.so usr/lib/libvosk.so
COPY vosk/lib/vosk_api.h usr/lib/vosk_api.h
WORKDIR /usr/src/verstappenbot
COPY --from=rust-builder /usr/src/verstappenbot/target/release/verstappenbot .
COPY max.mp3 .
COPY vosk/model/dutch vosk/model/dutch
ENV DISCORD_TOKEN=${DISCORD_TOKEN}
ENTRYPOINT ["./verstappenbot"]
