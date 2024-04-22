# A discord bot.
Bot listens -> Vosk speech to text model transcribes -> Bot plays a particular song if certain words are heard. 

I made this to get a laugh from my friends while watching f1 together. Code is a little messy. Currently, it should support working in multiple servers, but I haven't tested it. It's a little inefficent. I made it in one day before the 2024 Bahrain qualifiers.

# How to run

Install docker composer and use `docker compose up`! Don't forget to set `DISCORD_TOKEN` env variable. Or follow the following instructions.

Only external dependency you need is Opus codec that discord uses. If you are on linux/Mac, You can get it from your package manager. You need to manually build it on windows. Read the [original songbird repo](https://github.com/serenity-rs/songbird?tab=readme-ov-file#dependencies]) for more info.

After ensuring that can run it, to actually run it, change `.env.example` into `.env`, add your discord bot token, then just `cargo install just` and then type `just build` and `just run`. Otherwise, follow [this repo](https://github.com/Bear-03/vosk-rs?tab=readme-ov-file#compilation) 

Manually copying lib files to target dir makes cargo recompile it all. I might fix that in future. 

# I might update it in future. 
- I'm not satisfied with the code structure.
- I might make it public after solving inefficient parts.
- Make dev experience better (build script and copy without recompile)
  

TODO:
- Change serenity framework to poise.
- Proper error handling.
- Better connection handling. Currently we have to manually input channel id and bot won't be reset when disconnected.
