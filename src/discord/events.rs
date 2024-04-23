use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use serenity::{
    async_trait,
    client::{Context, EventHandler},
    model::{gateway::Ready, voice::VoiceState},
};

use songbird::{
    events::EventHandler as VoiceEventHandler,
    model::payload::{ClientDisconnect, Speaking},
    Event, EventContext,
};

use super::{audio_play::SongPlayer, ModelEntry, RecognitionEntries};
use crate::speech_to_text::SpeechToText;

pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
    async fn voice_state_update(
        &self,
        ctx: Context,
        old_voice_state: Option<VoiceState>,
        new_voice_state: VoiceState,
    ) {
        dbg!(new_voice_state.channel_id);
        dbg!(old_voice_state.map(|s| s.channel_id));
        dbg!(new_voice_state.user_id);
        dbg!(ctx.cache.current_user().id);

        if new_voice_state.user_id != ctx.cache.current_user().id {
            return;
        }

        // bot disconnected from voice channel
        if new_voice_state.channel_id.is_none() {
            tracing::warn!("Bot disconnected from voice channel. Attemting to remove handler.");
            if let (Some(songbird_client), Some(guild_id)) =
                (songbird::get(&ctx).await, new_voice_state.guild_id)
            {
                if let Err(e) = songbird_client.remove(guild_id).await {
                    tracing::warn!("Failed to remove handler: {:?}", e);
                }
            }
        }

    }
}

#[derive(Clone)]
pub struct Receiver {
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
