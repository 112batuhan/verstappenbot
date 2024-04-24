use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use serenity::{
    all::GuildId,
    async_trait,
    client::{Context, EventHandler},
    model::{gateway::Ready, voice::VoiceState},
};
use songbird::{
    events::EventHandler as VoiceEventHandler,
    id::ChannelId,
    model::payload::{ClientDisconnect, Speaking},
    Event, EventContext, Songbird,
};

use crate::speech_to_text::SpeechToText;

use super::{audio_play::SongPlayer, ModelEntry, RecognitionEntries};

pub fn check_if_channel_empty(ctx: &Context, guild_id: GuildId, channel_id: ChannelId) -> bool {
    let someone_there = ctx
        .cache
        .guild(guild_id)
        .unwrap()
        .voice_states
        .iter()
        .filter(|(_, state)| state.user_id != ctx.cache.current_user().id)
        .any(|(_id, state)| state.channel_id == Some(channel_id.0.into()));
    !someone_there
}

pub struct DefaultHandler {
    pub models: Arc<Vec<ModelEntry>>,
    pub songbird_client: Arc<Songbird>,
}

#[async_trait]
impl EventHandler for DefaultHandler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
    async fn voice_state_update(
        &self,
        ctx: Context,
        _: Option<VoiceState>,
        new_voice_state: VoiceState,
    ) {
        // Basically check if the channel that the bot is in is empty everytime someone joins or leaves.
        // To avoid deadlock, we have to call remove outside of the lock
        if let Some(guild_id) = new_voice_state.guild_id {
            let remove = if let Some(call_handler_lock) = self.songbird_client.get(guild_id) {
                let call_handler = call_handler_lock.lock().await;
                if let Some(current_channel) = call_handler.current_channel() {
                    if check_if_channel_empty(&ctx, guild_id, current_channel) {
                        tracing::info!(
                            "Removing call_handler because the channel is empty:{}-{:?}",
                            guild_id,
                            current_channel
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    // Remove the call_handler if it's not in a channel. This kills reconnect attemts
                    // But it's better to keep call_handlers in sync then to have a reconnect attempt
                    tracing::info!(
                        "Removing call_handler because it's not in a channel: {:?}",
                        guild_id
                    );
                    true
                }
            } else {
                false
            };
            if remove {
                if let Err(err) = self.songbird_client.remove(guild_id).await {
                    tracing::error!("Failed to remove call_handler: {:?}", err);
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct VoiceHandler {
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

impl VoiceHandler {
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

    fn get_speech_to_text_instances(&self) -> Vec<Mutex<SpeechToText>> {
        self.inner
            .models
            .iter()
            .filter_map(|model_entry| {
                let words = self.inner.words.filter_by_language(model_entry.language);
                let phrases = self.inner.phrases.filter_by_language(model_entry.language);
                if words.len() + phrases.len() == 0 {
                    None
                } else {
                    Some(Mutex::new(SpeechToText::new_with_grammar(
                        &model_entry.model,
                        model_entry.language,
                        &words,
                        &phrases,
                    )))
                }
            })
            .collect()
    }

    pub fn add_listener(&self, ssrc: u32, user_id: u64) {
        self.inner
            .listeners
            .insert(ssrc, self.get_speech_to_text_instances());
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
                listener.lock().unwrap().listen(audio);
            }
        }
    }

    pub fn reset_listeners(&self) {
        self.inner.listeners.clear();
        self.inner.user_ids.clear();
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
impl VoiceEventHandler for VoiceHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        use EventContext as Ctx;

        match ctx {
            Ctx::SpeakingStateUpdate(Speaking {
                speaking: _,
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
                        self.listen(*ssrc, decoded_voice);
                    }
                }
                for ssrc in &tick.silent {
                    self.finalise(*ssrc).await;
                }
            }

            Ctx::ClientDisconnect(ClientDisconnect { user_id, .. }) => {
                self.remove_listener(user_id.0)
            }
            Ctx::DriverDisconnect(disconnect_data) => {
                // This happens when the bot is disconnected or the bot is moved to another channel
                self.reset_listeners();
                if let Some(reason) = &disconnect_data.reason {
                    tracing::debug!("Driver disconnected: {:?}", reason);
                }
            }
            Ctx::DriverReconnect(reconnect_data) => {
                // Idk what causes this
                self.reset_listeners();
                tracing::warn!("Driver reconnected: {:?}", reconnect_data);
            }
            _ => {}
        }

        None
    }
}
