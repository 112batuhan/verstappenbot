use std::{
    env,
    sync::{Arc, Mutex},
};

use dashmap::DashMap;

use serenity::{
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
    model::{
        id::UserId,
        payload::{ClientDisconnect, Speaking},
    },
    packet::Packet,
    typemap::TypeMapKey,
    Config, CoreEvent, Event, EventContext, EventHandler as VoiceEventHandler, SerenityInit,
};

use vosk::Model;

use crate::{
    audio_play::{self, SongPlayer},
    speech_to_text::SpeechToText,
};

struct ModelKey;
impl TypeMapKey for ModelKey {
    type Value = Arc<Model>;
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

struct Receiver {
    inner: Arc<ReceiverInner>,
}

struct ReceiverInner {
    model: Arc<Model>,
    text_to_speech: DashMap<u32, Mutex<SpeechToText>>,
    user_ids: DashMap<u64, u32>,
    player: SongPlayer,
}

impl Receiver {
    pub fn new(model: Arc<Model>, player: SongPlayer) -> Self {
        Self {
            inner: Arc::new(ReceiverInner {
                model,
                text_to_speech: DashMap::new(),
                user_ids: DashMap::new(),
                player,
            }),
        }
    }

    pub fn add_listener(&self, ssrc: u32, user_id: u64) {
        self.inner.text_to_speech.insert(
            ssrc,
            Mutex::new(SpeechToText::new_with_grammar(
                &self.inner.model,
                &["intihar", "as kendini"],
            )),
        );
        self.inner.user_ids.insert(user_id, ssrc);
    }

    pub fn remove_listener(&self, user_id: u64) {
        let ssrc = self.inner.user_ids.remove(&user_id);
        if let Some(ssrc) = ssrc {
            self.inner.text_to_speech.remove(&ssrc.1);
        }
    }

    pub fn listen(&self, ssrc: u32, audio: &[i16]) {
        self.inner
            .text_to_speech
            .get(&ssrc)
            .map(|listener| listener.lock().unwrap().listen(&audio));
    }

    pub async fn finalise(&self, ssrc: u32) {
        let listener = self.inner.text_to_speech.get(&ssrc);
        if let Some(listener) = listener {
            let finalized = listener.lock().unwrap().finalise();
            if let Some(finalized) = finalized {
                self.inner.player.play_song(&finalized).await;
            }
        }
    }

    pub fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
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

    let model = Model::new("vosk/model/turkish").expect("Could not create the model");
    {
        let mut data = client.data.write().await;
        data.insert::<ModelKey>(Arc::new(model));
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

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    if let Ok(handler_lock) = manager.join(guild_id, connect_to).await {
        // NOTE: this skips listening for the actual connection result.
        let mut handler = handler_lock.lock().await;

        let mut player = audio_play::SongPlayer::new(manager.clone(), guild_id);
        player.add_song("intihar", "intihar.ogg").await;
        player.add_song("as", "as.mp3").await;

        let model = ctx.data.read().await.get::<ModelKey>().unwrap().clone();
        let evt_receiver = Receiver::new(model, player);

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
