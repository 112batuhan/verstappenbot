use super::check_msg;
use super::Context;
use super::RecognitionType;
use super::SoundBoard;

use crate::speech_to_text::ModelLanguage;

use anyhow::Result;
use serenity::all::ChannelId;
use serenity::all::Mentionable;
use songbird::CoreEvent;

#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn ping(ctx: Context<'_>) -> Result<()> {
    check_msg(ctx.say("Pong!").await);
    Ok(())
}

/// Use channel Id
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn join(ctx: Context<'_>) -> Result<()> {
    let (guild_id, channel_id) = {
        // guild_id is guaranteed to be Some because this is a guild_only command
        let guild = ctx.guild().unwrap();
        let channel_id: Option<ChannelId> = guild
            .voice_states
            .get(&ctx.author().id)
            .and_then(|voice_state| voice_state.channel_id);

        (guild.id, channel_id)
    };

    tracing::info!("joining {}:{:?}", &guild_id, &channel_id);

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            check_msg(ctx.reply("Not in a voice channel").await);
            return Ok(());
        }
    };

    let songbird_client = &ctx.data().songbird;

    if let Some(call_handler) = songbird_client.get(guild_id) {
        if call_handler.lock().await.current_channel() == Some(connect_to.into()) {
            check_msg(ctx.reply("Already in your channel").await);
            return Ok(());
        }
    }

    match songbird_client.join(guild_id, connect_to).await {
        Ok(call_handler_lock) => {
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
            let models = ctx.data().models.clone();

            let voice_handler = sound_board.get_voice_handler(models, player);

            {
                let mut call_handler = call_handler_lock.lock().await;
                // just to make sure we don't have multiple event listeners
                call_handler.remove_all_global_events();

                call_handler
                    .add_global_event(CoreEvent::SpeakingStateUpdate.into(), voice_handler.clone());
                call_handler
                    .add_global_event(CoreEvent::ClientDisconnect.into(), voice_handler.clone());
                call_handler.add_global_event(CoreEvent::VoiceTick.into(), voice_handler.clone());
                call_handler
                    .add_global_event(CoreEvent::DriverDisconnect.into(), voice_handler.clone());
                call_handler.add_global_event(CoreEvent::DriverReconnect.into(), voice_handler);
            }
            check_msg(ctx.reply(format!("Joined {}", connect_to.mention())).await);
        }
        Err(e) => {
            tracing::error!("Error joining channel: {:?}", e);
            check_msg(ctx.reply(format!("Failed: {:?}", e)).await);
        }
    }

    Ok(())
}

#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn leave(ctx: Context<'_>) -> Result<()> {
    let guild_id = ctx.guild_id().unwrap();

    let songbird_client = &ctx.data().songbird;
    let has_call_handler = songbird_client.get(guild_id).is_some();

    if has_call_handler {
        if let Err(e) = songbird_client.leave(guild_id).await {
            tracing::error!("Error leaving channel: {:?}", e);
            check_msg(ctx.reply(format!("Failed: {:?}", e)).await);
        }

        check_msg(ctx.reply("Left voice channel").await);
    } else {
        check_msg(ctx.reply("Not in a voice channel").await);
    }

    Ok(())
}
