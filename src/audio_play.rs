use std::{collections::HashMap, sync::Arc};

use serenity::all::GuildId;
use songbird::{
    driver::Bitrate,
    input::{cached::Compressed, File},
    Songbird,
};

pub struct SongPlayer {
    pub songs: HashMap<String, Compressed>,
    pub client: Arc<Songbird>,
    pub guild_id: GuildId,
}
impl SongPlayer {
    pub fn new(client: Arc<Songbird>, guild_id: GuildId) -> Self {
        Self {
            songs: HashMap::new(),
            client,
            guild_id,
        }
    }
    pub async fn add_song(&mut self, name: &str, song_path: &str) {
        let src = Compressed::new(
            File::new(song_path.to_string()).into(),
            Bitrate::BitsPerSecond(193_000),
        )
        .await
        .expect("These parameters are well-defined.");
        let loader_handler = src.raw.spawn_loader();
        self.songs.insert(name.to_string(), src);
        let _ = loader_handler.join();
    }

    pub async fn play_song(&self, name: &str) {
        if let Some(source) = self.songs.get(name) {
            if let Some(handler_lock) = self.client.get(self.guild_id) {
                let mut handler = handler_lock.lock().await;

                let _sound = handler.play_input(source.new_handle().into());
            }
        }
    }
}
