use std::sync::Arc;

use super::check_msg;
use super::Context;
use super::RecognitionType;
use super::SoundBoard;

use crate::speech_to_text::ModelLanguage;

use anyhow::Result;
use serenity::all::Attachment;
use serenity::all::ChannelId;
use serenity::all::Mentionable;
use songbird::Call;
use songbird::CoreEvent;
use songbird::Songbird;
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use uuid::Uuid;

#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn ping(ctx: Context<'_>) -> Result<()> {
    check_msg(ctx.say("Pong!").await);
    Ok(())
}

#[poise::command(prefix_command, owners_only)]
pub async fn register(ctx: Context<'_>) -> Result<()> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

#[poise::command(prefix_command, track_edits, slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"] command: Option<String>,
) -> Result<()> {
    let config = poise::builtins::HelpConfiguration {
        ..Default::default()
    };
    poise::builtins::help(ctx, command.as_deref(), config).await?;
    Ok(())
}

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
            initiate_handler(&ctx, songbird_client.clone(), call_handler_lock).await?;
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
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn add_sound(
    ctx: Context<'_>,
    #[description = "Prompt for the sound. Add phrases instead of single words to reduce false positives."]
    prompt: String,
    #[description = "Language of the prompt"] language: String,
    #[description = "Sound you want to add"] attachment: Attachment,
) -> Result<()> {
    let content = match attachment.download().await {
        Ok(content) => content,
        Err(why) => {
            tracing::error!("Error downloading attachment: {:?}", why);
            let _ = ctx.reply("Error downloading attachment").await;
            return Ok(());
        }
    };

    if attachment.size > 2 * 1024 * 1024 {
        let _ = ctx.reply("File size too large. Max 2mb.").await;
        return Ok(());
    }

    if !attachment
        .content_type
        .is_some_and(|x| x.starts_with("audio"))
    {
        let _ = ctx.reply("Only audio files are supported").await;
        return Ok(());
    }

    let id = Uuid::new_v4();
    let mut file = match File::create(format!("./songs/{}", id)).await {
        Ok(file) => file,
        Err(why) => {
            tracing::error!("Error creating file: {:?}", why);
            let _ = ctx.reply("Error creating attachment").await;
            return Ok(());
        }
    };

    if let Err(why) = file.write_all(&content).await {
        println!("Error writing to file: {:?}", why);
        return Ok(());
    }

    let trimmed_prompt = prompt.trim();
    if trimmed_prompt.is_empty() {
        let _ = ctx.reply("Prompt cannot be empty").await;
        return Ok(());
    }

    if ModelLanguage::from_str(&language).is_none() {
        let _ = ctx.reply("Invalid language").await;
        return Ok(());
    }

    ctx.data()
        .database
        .add_sound(
            &ctx.guild_id().unwrap().to_string(),
            trimmed_prompt,
            &language,
            &id.to_string(),
        )
        .await?;

    let _ = ctx
        .reply(&format!(
            "Saved {}, use join command to invite the bot.",
            attachment.filename
        ))
        .await;
    ctx.data().songbird.leave(ctx.guild_id().unwrap()).await?;

    Ok(())
}

#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn remove_sound(ctx: Context<'_>, prompt: String) -> Result<()> {
    let trimmed_prompt = prompt.trim();
    if trimmed_prompt.is_empty() {
        let _ = ctx.reply("Prompt cannot be empty").await;
        return Ok(());
    }

    let deleted = ctx
        .data()
        .database
        .remove_sound(&ctx.guild_id().unwrap().to_string(), trimmed_prompt)
        .await?;

    if let Err(err) = fs::remove_file(format!("songs/{}", deleted.file_name)).await {
        tracing::error!("Error removing sound: {:?}", err);
        let _ = ctx.reply("Error removing sound").await;
        return Ok(());
    }

    let _ = ctx
        .reply(&format!(
            "Removed {}, use join command to invite the bot.",
            deleted.file_name
        ))
        .await;
    ctx.data().songbird.leave(ctx.guild_id().unwrap()).await?;

    Ok(())
}

#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn list_sounds(ctx: Context<'_>) -> Result<()> {
    let sounds = ctx
        .data()
        .database
        .get_sounds(ctx.guild_id().unwrap().to_string().as_str())
        .await?;

    let sounds = sounds
        .into_iter()
        .map(|sound| format!("{} - {}", sound.prompt, sound.language))
        .collect::<Vec<String>>()
        .join("\n");

    let sounds = if sounds.is_empty() {
        "No sounds found".to_string()
    } else {
        format!("```{}```", sounds)
    };

    let _ = ctx.reply(sounds).await;

    Ok(())
}

async fn initiate_handler(
    ctx: &Context<'_>,
    songbird: Arc<Songbird>,
    call_handler_lock: Arc<Mutex<Call>>,
) -> Result<()> {
    let guild_id = ctx.guild_id().unwrap();
    let sounds = ctx
        .data()
        .database
        .get_sounds(guild_id.to_string().as_str())
        .await?;

    let sound_board = sounds
        .into_iter()
        .fold(SoundBoard::new(), |sound_board, sound| {
            let recognition_type = if sound.prompt.split_whitespace().count() > 1 {
                RecognitionType::PHRASE
            } else {
                RecognitionType::WORD
            };
            sound_board.add_song(
                &sound.prompt,
                recognition_type,
                ModelLanguage::from_str(&sound.language).unwrap(),
                format!("songs/{}", sound.file_name).as_str(),
            )
        });

    let player = sound_board.get_player(songbird.clone(), guild_id).await;
    let models = ctx.data().models.clone();

    let voice_handler = sound_board.get_voice_handler(models, player);

    {
        let mut call_handler = call_handler_lock.lock().await;

        call_handler.remove_all_global_events();

        call_handler.add_global_event(CoreEvent::SpeakingStateUpdate.into(), voice_handler.clone());
        call_handler.add_global_event(CoreEvent::ClientDisconnect.into(), voice_handler.clone());
        call_handler.add_global_event(CoreEvent::VoiceTick.into(), voice_handler.clone());
        call_handler.add_global_event(CoreEvent::DriverDisconnect.into(), voice_handler.clone());
        call_handler.add_global_event(CoreEvent::DriverReconnect.into(), voice_handler);
    }
    Ok(())
}
