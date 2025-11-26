use std::{
    collections::HashMap,
    env,
    path::PathBuf,
    time::{Duration, UNIX_EPOCH},
};

use directories::ProjectDirs;
use evalexpr::{DefaultNumericTypes, HashMapContext, eval_number_with_context_mut};
use poise::{
    CreateReply, command,
    serenity_prelude::{
        self as serenity, ChannelId, CreateAllowedMentions, CreateEmbed, CreateMessage,
        GatewayIntents, GuildId, Mentionable, Message, RoleId, UserId, async_trait,
        colours::css::DANGER,
    },
};
use poise_error::anyhow::{self, Context};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

const GUILD_ID: GuildId = GuildId::new(1250948547403055114);
const COUNTING_CHANNEL_ID: ChannelId = ChannelId::new(1436039934292262924);
const DUMBASS_ROLE_ID: RoleId = RoleId::new(1433551900233564371);
const DUMBASS_ROLE_TIMEOUT_LEN: Duration = Duration::from_secs(60 * 60);
const UNIX_EPOCH_ELAPSED_ERROR: &str = "time travel is real";

struct EventHandler;

#[async_trait]
impl serenity::EventHandler for EventHandler {
    async fn message(&self, ctx: serenity::Context, new_message: Message) {
        let result: anyhow::Result<_> = async move {
            if new_message.channel_id == COUNTING_CHANNEL_ID {
                let mut eval_context = HashMapContext::<DefaultNumericTypes>::new();
                let mut data = Data::get();

                if !new_message.author.bot
                    && let Ok(eval_result) = eval_number_with_context_mut(
                        &new_message.content.replace('\\', ""),
                        &mut eval_context,
                    )
                {
                    let eval_result = eval_result.trunc() as i64;

                    if eval_result == data.last_number + 1
                        && data
                            .last_user_id
                            .is_none_or(|last_user_id| new_message.author.id != last_user_id)
                    {
                        new_message
                            .react(&ctx, '✅')
                            .await
                            .context("failed to react to message")?;
                        info!(
                            "{} correctly counted {eval_result}",
                            new_message.author.tag(),
                        );

                        data.last_number += 1;

                        if data.last_number > data.high_score {
                            data.high_score = data.last_number;
                        }

                        *data.leaderboard.entry(new_message.author.id).or_default() += 1;
                    } else {
                        new_message
                            .react(&ctx, '❌')
                            .await
                            .context("failed to react to message")?;
                        new_message
                            .member(&ctx)
                            .await
                            .context("failed to get member")?
                            .add_role(&ctx, DUMBASS_ROLE_ID)
                            .await
                            .context("failed to add dumbass role to member")?;

                        let how_messed_up = if eval_result != data.last_number + 1 {
                            info!(
                                "{} incorrectly counted {eval_result}",
                                new_message.author.tag(),
                            );

                            format!(
                                "wrong. The next number was supposed to be **{}**",
                                data.last_number + 1,
                            )
                        } else
                        // if same user as last count
                        {
                            info!("{} counted twice in a row", new_message.author.tag());

                            "twice in a row".to_string()
                        };

                        new_message
                            .channel_id
                            .send_message(
                                &ctx,
                                CreateMessage::new().reference_message(&new_message).embed(
                                    CreateEmbed::new()
                                        .title("Count Reset")
                                        .description(format!(
                                            "{} messed up the count by counting {how_messed_up}.",
                                            new_message.author.mention(),
                                        ))
                                        .color(DANGER),
                                ),
                            )
                            .await
                            .context("failed to send failure message")?;
                        data.dumbass_role_timeouts.insert(
                            new_message.author.id,
                            (UNIX_EPOCH.elapsed().expect(UNIX_EPOCH_ELAPSED_ERROR)
                                + DUMBASS_ROLE_TIMEOUT_LEN)
                                .as_secs(),
                        );

                        data.last_number = 0;
                    }

                    data.last_user_id = Some(new_message.author.id);

                    data.set().context("failed to set data")?;
                }
            }

            Ok(())
        }
        .await;

        if let Err(err) = result {
            error!("Failed to handle message event: {err:#}");
        }
    }
}

#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
struct Data {
    last_number: i64,
    last_user_id: Option<UserId>,
    high_score: i64,
    leaderboard: HashMap<UserId, u64>,
    dumbass_role_timeouts: HashMap<UserId, u64>,
}

impl Data {
    fn get() -> Self {
        std::fs::File::open(Self::path())
            .ok()
            .and_then(|file| ciborium::from_reader(file).ok())
            .unwrap_or_default()
    }

    fn set(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(Self::project_dirs().data_dir())?;
        ciborium::into_writer(self, std::fs::File::create(Self::path())?)?;

        Ok(())
    }

    fn path() -> PathBuf {
        Self::project_dirs().data_dir().join("data.cbor")
    }

    fn project_dirs() -> ProjectDirs {
        ProjectDirs::from("com", "valentinegb", "goober-counting")
            .expect("no valid home directory path could be retrieved from the operating system")
    }
}

#[tokio::main]
async fn main() {
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("goober_counting=debug,info"))
        .unwrap();
    let registry = tracing_subscriber::registry().with(filter_layer);

    match tracing_journald::layer() {
        Ok(journald_layer) => {
            registry.with(journald_layer).init();
        }
        Err(_) => {
            let fmt_layer = tracing_subscriber::fmt::layer().with_target(false);

            registry.with(fmt_layer).init();
        }
    }

    if let Err(err) = try_main().await {
        error!("Fatal error: {err:#}");
    }
}

async fn try_main() -> anyhow::Result<()> {
    info!("Starting up");

    #[cfg(debug_assertions)]
    if let Err(err) = dotenvy::dotenv() {
        tracing::warn!("Could not load `.env` file: {err:#}");
    }

    let mut client = serenity::Client::builder(
        env::var("GOOBER_COUNTING_DISCORD_TOKEN")?,
        GatewayIntents::MESSAGE_CONTENT | GatewayIntents::GUILD_MESSAGES,
    )
    .framework(
        poise::Framework::builder()
            .options(poise::FrameworkOptions {
                commands: vec![leaderboard()],
                on_error: poise_error::on_error,
                pre_command: |ctx: poise_error::Context| {
                    Box::pin(async move {
                        info!("{} invoked {}", ctx.author().tag(), ctx.invocation_string());
                    })
                },
                ..Default::default()
            })
            .setup(|ctx, ready, framework| {
                Box::pin(async move {
                    info!("Logged in as {}", ready.user.tag());
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    info!("Registered commands");
                    tokio::spawn(dumbass_role_loop(ctx.clone()));

                    Ok(())
                })
            })
            .build(),
    )
    .event_handler(EventHandler)
    .await?;

    client.start_autosharded().await?;
    info!("Shutting down");

    Ok(())
}

async fn dumbass_role_loop(ctx: serenity::Context) {
    loop {
        let mut data = Data::get();
        let mut changed = false;
        let now = UNIX_EPOCH
            .elapsed()
            .expect(UNIX_EPOCH_ELAPSED_ERROR)
            .as_secs();

        for (user_id, timeout) in data.dumbass_role_timeouts.clone() {
            if timeout <= now {
                if let Ok(member) = GUILD_ID.member(&ctx, user_id).await
                    && let Err(err) = member.remove_role(&ctx, DUMBASS_ROLE_ID).await
                {
                    error!("Failed to remove role: {err:#}");
                }

                data.dumbass_role_timeouts.remove(&user_id);
                changed = true;

                info!("Removed dumbass role from a user with the ID '{user_id}'");
            }
        }

        if changed && let Err(err) = data.set() {
            error!("Failed to set data: {err:#}");
        }

        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

/// Shows how many numbers people have counted.
#[command(slash_command)]
async fn leaderboard(ctx: poise_error::Context<'_>) -> anyhow::Result<()> {
    let mut leaderboard: Vec<(UserId, u64)> = Data::get().leaderboard.into_iter().collect();
    let mut description = String::new();

    leaderboard.sort_by(|(_, a), (_, b)| b.cmp(a));

    for (user_id, numbers) in leaderboard {
        description += &format!("1. {}: {numbers}\n", user_id.mention());
    }

    ctx.send(
        CreateReply::default()
            .embed(
                CreateEmbed::new()
                    .title("Leaderboard")
                    .description(description),
            )
            .allowed_mentions(CreateAllowedMentions::new()),
    )
    .await?;

    Ok(())
}
