//! It's a bit complicated to keep track of the [`songbird::handler::Call`] and [`events::VoiceHandler`] in sync.
//! Here's a brief explanation of how it works:
//!
//! - Join is handled in [`commands::join()`]. This adds a new [`events::VoiceHandler`] handler
//!   to the [`songbird::handler::Call`] handler which is managed by [`songbird::manager::Songbird`].
//!
//! - If [`commands::join()`] command is used when bot is already connected to a channel
//!   old [`events::VoiceHandler`] handler is removed and new handler is added to the [`songbird::handler::Call`].
//!
//! - If bot is dragged into a new channel (aka channel movement), then joining a new channel doesn't replace the [`events::VoiceHandler`].
//!   Instead the recognizers and user id's that are in the [`events::VoiceHandler`] gets reset with [`events::VoiceHandler::reset_listeners()`]
//!
//! - [`commands::leave()`] command only disconnects the bot. It doesn't drop the [`songbird::handler::Call`]
//!   hence [`events::VoiceHandler`] also stays.
//!   This also triggers `voice_state_update` event in [`events::DefaultHandler`].
//!   Which will eventually drop the [`songbird::handler::Call`]. (Explained in the next point)
//!
//! - [`songbird::handler::Call`] is dropped by checking if the bot is alone or not in a channel when
//!   `voice_state_update` event is fired in [`events::DefaultHandler`].
//!   If that's the case [`songbird::handler::Call`] is removed by calling [`songbird::manager::Songbird::remove()`].
//!   This also drops the [`events::VoiceHandler`].
//!
//! Sounds robust. Until something will eventually break as always.

use self::{audio_play::SongPlayer, events::VoiceHandler};
use std::{env, sync::Arc};

use poise::{Framework, FrameworkOptions, PrefixFrameworkOptions};

use serenity::all::{GatewayIntents, GuildId};
use songbird::{driver::DecodeMode, Config, Songbird};

use vosk::Model;

use crate::{discord::events::DefaultHandler, speech_to_text::ModelLanguage};

pub mod audio_play;
pub mod commands;
pub mod events;

pub struct Sound {
    name: String,
    recognition_type: RecognitionType,
    language: ModelLanguage,
    path: String,
}
pub struct SoundBoard {
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

    pub fn get_voice_handler(
        &self,
        models: Arc<Vec<ModelEntry>>,
        player: SongPlayer,
    ) -> VoiceHandler {
        let phrases = self.get_phrases();
        let words = self.get_words();
        VoiceHandler::new(models, player, words, phrases)
    }
}

pub enum RecognitionType {
    WORD,
    PHRASE,
}

pub struct RecognitionEntries {
    inner: Vec<RecognitionEntry>,
}
pub struct RecognitionEntry {
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

pub struct ModelEntry {
    pub model: Model,
    pub language: ModelLanguage,
}

pub struct Data {
    songbird: Arc<songbird::Songbird>,
    models: Arc<Vec<ModelEntry>>,
}

type Context<'a> = poise::Context<'a, Data, anyhow::Error>;

pub async fn run() {
    tracing_subscriber::fmt::init();

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let songbird_config = Config::default().decode_mode(DecodeMode::Decode);
    let songbird_client = Songbird::serenity_from_config(songbird_config);

    let framework_options = FrameworkOptions {
        commands: vec![commands::join(), commands::leave(), commands::ping()],
        prefix_options: PrefixFrameworkOptions {
            prefix: Some(".".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

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
    let models = Arc::new(models);
    let models_clone = models.clone();

    let songbird_client_clone = songbird_client.clone();
    let framework = Framework::new(framework_options, |_, _, _| {
        Box::pin(async {
            Ok(Data {
                songbird: songbird_client_clone,
                models: models_clone,
            })
        })
    });

    let songbird_client_clone = songbird_client.clone();
    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let mut client = serenity::Client::builder(&token, intents)
        .voice_manager_arc(songbird_client)
        .event_handler(DefaultHandler {
            models,
            songbird_client: songbird_client_clone,
        })
        .framework(framework)
        .await
        .expect("Err creating client");

    tokio::spawn(async move {
        let _ = client
            .start()
            .await
            .map_err(|why| println!("Client ended: {:?}", why));
    });

    let _signal_err = tokio::signal::ctrl_c().await;
    println!("Received Ctrl-C, shutting down.");
}

/// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg<T>(result: serenity::Result<T>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
