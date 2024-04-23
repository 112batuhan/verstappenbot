use std::{
    env,
    sync::{Arc, Mutex},
};

use dashmap::DashMap;

use serenity::{
    all::GuildId,
    async_trait,
    client::{Client, Context, EventHandler},
    framework::{
        standard::{
            macros::{command, group},
            Args, CommandResult, Configuration,
        },
        StandardFramework,
    },
    model::{channel::Message, gateway::Ready, id::ChannelId},
    prelude::{GatewayIntents, Mentionable},
    Result as SerenityResult,
};

use songbird::{
    driver::DecodeMode,
    model::payload::{ClientDisconnect, Speaking},
    typemap::TypeMapKey,
    Config, CoreEvent, Event, EventContext, EventHandler as VoiceEventHandler, SerenityInit,
    Songbird,
};

use vosk::Model;

use crate::{
    audio_play::SongPlayer,
    speech_to_text::{ModelLanguage, SpeechToText},
};

struct Sound {
    name: String,
    recognition_type: RecognitionType,
    language: ModelLanguage,
    path: String,
}
struct SoundBoard {
    sounds: Vec<Sound>,
}

impl SoundBoard {
    pub fn new() -> Self {
        Self { sounds: Vec::new() }
    }
    pub fn add_song(
        mut self,
        name: &str,
        recognition_type: RecognitionType,
        language: ModelLanguage,
        path: &str,
    ) -> Self {
        self.sounds.push(Sound {
            name: name.to_string(),
            recognition_type,
            language,
            path: path.to_string(),
        });
        self
    }
    pub async fn get_player(&self, client: Arc<Songbird>, guild_id: GuildId) -> SongPlayer {
        let mut player = SongPlayer::new(client, guild_id);
        for sound in &self.sounds {
            player
                .add_song(&sound.name, sound.language, &sound.path)
                .await;
        }
        player
    }

    pub fn get_phrases(&self) -> RecognitionEntries {
        let phrases = self
            .sounds
            .iter()
            .filter_map(|sound| {
                if let RecognitionType::PHRASE = sound.recognition_type {
                    Some(RecognitionEntry {
                        content: sound.name.clone(),
                        language: sound.language,
                    })
                } else {
                    None
                }
            })
            .collect();

        RecognitionEntries { inner: phrases }
    }
    pub fn get_words(&self) -> RecognitionEntries {
        let words = self
            .sounds
            .iter()
            .filter_map(|sound| {
                if let RecognitionType::WORD = sound.recognition_type {
                    Some(RecognitionEntry {
                        content: sound.name.clone(),
                        language: sound.language,
                    })
                } else {
                    None
                }
            })
            .collect();

        RecognitionEntries { inner: words }
    }

    pub fn get_receiver(&self, models: Arc<Vec<ModelEntry>>, player: SongPlayer) -> Receiver {
        let phrases = self.get_phrases();
        let words = self.get_words();
        Receiver::new(models, player, words, phrases)
    }
}

enum RecognitionType {
    WORD,
    PHRASE,
}

struct RecognitionEntries {
    inner: Vec<RecognitionEntry>,
}
struct RecognitionEntry {
    content: String,
    language: ModelLanguage,
}

impl RecognitionEntries {
    pub fn filter_by_language(&self, language: ModelLanguage) -> Vec<String> {
        self.inner
            .iter()
            .filter_map(|entry| {
                if entry.language == language {
                    Some(entry.content.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}

struct ModelEntry {
    model: Model,
    language: ModelLanguage,
}

struct ModelKey;
impl TypeMapKey for ModelKey {
    type Value = Arc<Vec<ModelEntry>>;
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[derive(Clone)]
struct Receiver {
    inner: Arc<ReceiverInner>,
}

struct ReceiverInner {
    models: Arc<Vec<ModelEntry>>,
    listeners: DashMap<u32, Vec<Mutex<SpeechToText>>>,
    user_ids: DashMap<u64, u32>,
    player: SongPlayer,
    phrases: RecognitionEntries,
    words: RecognitionEntries,
}

impl Receiver {
    pub fn new(
        models: Arc<Vec<ModelEntry>>,
        player: SongPlayer,
        words: RecognitionEntries,
        phrases: RecognitionEntries,
    ) -> Self {
        Self {
            inner: Arc::new(ReceiverInner {
                models,
                listeners: DashMap::new(),
                user_ids: DashMap::new(),
                player,
                words,
                phrases,
            }),
        }
    }

    pub fn add_listener(&self, ssrc: u32, user_id: u64) {
        let speech_to_text_instances = self
            .inner
            .models
            .iter()
            .map(|model_entry| {
                let words = self.inner.words.filter_by_language(model_entry.language);
                let phrases = self.inner.phrases.filter_by_language(model_entry.language);
                Mutex::new(SpeechToText::new_with_grammar(
                    &model_entry.model,
                    model_entry.language,
                    &words,
                    &phrases,
                ))
            })
            .collect();

        self.inner.listeners.insert(ssrc, speech_to_text_instances);
        self.inner.user_ids.insert(user_id, ssrc);
    }

    pub fn remove_listener(&self, user_id: u64) {
        let ssrc = self.inner.user_ids.remove(&user_id);
        if let Some(ssrc) = ssrc {
            self.inner.listeners.remove(&ssrc.1);
        }
    }

    pub fn listen(&self, ssrc: u32, audio: &[i16]) {
        if let Some(listeners) = self.inner.listeners.get(&ssrc) {
            for listener in listeners.iter() {
                listener.lock().unwrap().listen(&audio);
            }
        }
    }

    pub async fn finalise(&self, ssrc: u32) {
        if let Some(listeners) = self.inner.listeners.get(&ssrc) {
            for listener in listeners.iter() {
                let finalized = listener.lock().unwrap().finalise();
                if let Some((finalized, language)) = finalized {
                    self.inner.player.play_song(&finalized, language).await;
                }
            }
        }
    }
}

#[async_trait]
impl VoiceEventHandler for Receiver {
    #[allow(unused_variables)]
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        use EventContext as Ctx;

        match ctx {
            Ctx::SpeakingStateUpdate(Speaking {
                speaking,
                ssrc,
                user_id,
                ..
            }) => {
                if let Some(user_id) = user_id {
                    self.add_listener(*ssrc, user_id.0)
                }
            }
            Ctx::VoiceTick(tick) => {
                for (ssrc, data) in &tick.speaking {
                    if let Some(decoded_voice) = data.decoded_voice.as_ref() {
                        self.listen(*ssrc, &decoded_voice);
                    }
                }
                for ssrc in &tick.silent {
                    self.finalise(*ssrc).await;
                }
            }

            Ctx::ClientDisconnect(ClientDisconnect { user_id, .. }) => {
                self.remove_listener(user_id.0)
            }
            _ => {}
        }

        None
    }
}

#[group]
#[commands(join, leave, ping)]
struct General;

pub async fn run() {
    tracing_subscriber::fmt::init();

    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let framework = StandardFramework::new().group(&GENERAL_GROUP);
    framework.configure(Configuration::new().prefix("."));

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    // Here, we need to configure Songbird to decode all incoming voice packets.
    // If you want, you can do this on a per-call basis---here, we need it to
    // read the audio data that other people are sending us!
    let songbird_config = Config::default().decode_mode(DecodeMode::Decode);

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .framework(framework)
        .register_songbird_from_config(songbird_config)
        .await
        .expect("Err creating client");

    let models = vec![
        ModelEntry {
            model: Model::new("vosk/model/turkish").expect("Could not create the model"),
            language: ModelLanguage::TURKISH,
        },
        ModelEntry {
            model: Model::new("vosk/model/dutch").expect("Could not create the model"),
            language: ModelLanguage::DUTCH,
        },
    ];

    {
        let mut data = client.data.write().await;
        data.insert::<ModelKey>(Arc::new(models));
    }

    let _ = client
        .start()
        .await
        .map_err(|why| println!("Client ended: {:?}", why));
}
#[command]
#[only_in(guilds)]
async fn join(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    println!("Joining voice channel");
    let Ok(connect_to) = args.single::<ChannelId>() else {
        check_msg(
            msg.reply(ctx, "Requires a valid voice channel ID be given")
                .await,
        );

        return Ok(());
    };

    let guild_id = msg.guild_id.unwrap();

    let songbird_client = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    if let Ok(handler_lock) = songbird_client.join(guild_id, connect_to).await {
        // NOTE: this skips listening for the actual connection result.
        let mut handler = handler_lock.lock().await;

        let sound_board = SoundBoard::new()
            .add_song(
                "intihar",
                RecognitionType::WORD,
                ModelLanguage::TURKISH,
                "intihar.ogg",
            )
            .add_song(
                "as kendini",
                RecognitionType::PHRASE,
                ModelLanguage::TURKISH,
                "as.mp3",
            )
            .add_song(
                "verstappen",
                RecognitionType::WORD,
                ModelLanguage::DUTCH,
                "max.mp3",
            );

        let player = sound_board
            .get_player(songbird_client.clone(), guild_id)
            .await;
        let model = ctx.data.read().await.get::<ModelKey>().unwrap().clone();

        let evt_receiver = sound_board.get_receiver(model, player);

        handler.add_global_event(CoreEvent::SpeakingStateUpdate.into(), evt_receiver.clone());
        handler.add_global_event(CoreEvent::ClientDisconnect.into(), evt_receiver.clone());
        handler.add_global_event(CoreEvent::VoiceTick.into(), evt_receiver);

        check_msg(
            msg.channel_id
                .say(&ctx.http, &format!("Joined {}", connect_to.mention()))
                .await,
        );
    } else {
        check_msg(
            msg.channel_id
                .say(&ctx.http, "Error joining the channel")
                .await,
        );
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn leave(ctx: &Context, msg: &Message) -> CommandResult {
    let guild_id = msg.guild_id.unwrap();

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();
    let has_handler = manager.get(guild_id).is_some();

    if has_handler {
        if let Err(e) = manager.remove(guild_id).await {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, format!("Failed: {:?}", e))
                    .await,
            );
        }

        check_msg(msg.channel_id.say(&ctx.http, "Left voice channel").await);
    } else {
        check_msg(msg.reply(ctx, "Not in a voice channel").await);
    }

    Ok(())
}

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    check_msg(msg.channel_id.say(&ctx.http, "Pong!").await);

    Ok(())
}

/// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
