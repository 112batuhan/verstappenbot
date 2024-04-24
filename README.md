# A discord bot.
Bot listens -> Vosk speech to text model transcribes -> Bot plays a particular song if certain words are heard. 

I made this to get a laugh from my friends while watching f1 together. It used to support only one prompt. I liked the idea so much, I made it into a bot that is usable by everyone.

# How to run

!! A bit outdated. I will update with more details later.

Install docker composer and use `docker compose up`! Don't forget to set `DISCORD_TOKEN`, `DATABASE_URL` and `OWNER_ID` env variables.

Only external dependency you need is Opus codec that discord uses. If you are on linux/Mac, You can get it from your package manager. You need to manually build it on windows. Read the [original songbird repo](https://github.com/serenity-rs/songbird?tab=readme-ov-file#dependencies]) for more info.

After ensuring that can run it, to actually run it, change `.env.example` into `.env`, add your discord bot token, then just `cargo install just` and then type `just build` and `just run`. Otherwise, follow [this repo](https://github.com/Bear-03/vosk-rs?tab=readme-ov-file#compilation) 

Manually copying lib files to target dir makes cargo recompile it all. I might fix that in future. 

TODO:
- Make dev experience better (build script and copy without recompile)
- Checking the integrity of the audio files
- Compress files before storing
- Better audio list with buttons etc
- Preview

Future Plans:
- Website for uploading and managing sounds.
