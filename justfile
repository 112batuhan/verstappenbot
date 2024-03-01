set dotenv-load

build:
     cargo build

run:
    cp -u vosk/lib/libvosk.so target/libvosk.so
    cp -u vosk/lib/vosk_api.h target/vosk_api.h
    cargo run
