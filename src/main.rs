pub mod audio_play;
pub mod discord;
pub mod speech_to_text;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    discord::run().await;
}
