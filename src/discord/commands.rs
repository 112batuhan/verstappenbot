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
    println!("Joining voice channel");

    let (guild_id, channel_id) = {
        let guild = ctx.guild().unwrap();
        let channel_id: Option<ChannelId> = guild
            .voice_states
            .get(&ctx.author().id)
            .and_then(|voice_state| voice_state.channel_id);

        (guild.id, channel_id)
    };

    let connected_channel: Option<ChannelId> = ctx
        .guild()
        .unwrap()
        .voice_states
        .get(&ctx.framework().bot_id)
        .and_then(|voice_state| voice_state.channel_id);

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            check_msg(ctx.reply("Not in a voice channel").await);
            return Ok(());
        }
    };

    if let Some(connected_channel) = connected_channel {
        if connected_channel == connect_to {
            check_msg(ctx.reply("Already in that voice channel").await);
            return Ok(());
        }
    }

    let songbird_client = &ctx.data().songbird;

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
        let models = ctx.data().models.clone();

        let evt_receiver = sound_board.get_receiver(models, player);

        handler.add_global_event(CoreEvent::SpeakingStateUpdate.into(), evt_receiver.clone());
        handler.add_global_event(CoreEvent::ClientDisconnect.into(), evt_receiver.clone());
        handler.add_global_event(CoreEvent::VoiceTick.into(), evt_receiver);

        check_msg(ctx.say(format!("Joined {}", connect_to.mention())).await);
    } else {
        check_msg(ctx.say("Error joining the channel").await);
    }

    Ok(())
}

#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn leave(ctx: Context<'_>) -> Result<()> {
    let guild_id = ctx.guild_id().unwrap();

    let songbird_client = &ctx.data().songbird;
    let has_handler = songbird_client.get(guild_id).is_some();

    if has_handler {
        if let Err(e) = songbird_client.remove(guild_id).await {
            check_msg(ctx.say(format!("Failed: {:?}", e)).await);
        }

        check_msg(ctx.say("Left voice channel").await);
    } else {
        check_msg(ctx.say("Not in a voice channel").await);
    }

    Ok(())
}
