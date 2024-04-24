#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    verstappenbot::discord_bot::run().await;
}
