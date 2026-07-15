use crate::models::{AppSettings, VisualNovel};
use chrono::{DateTime, Utc};
use discord_rich_presence::error::Error as DiscordError;
use discord_rich_presence::{DiscordIpc, DiscordIpcClient, activity};
const DISCORD_APP_ID: &str = "1512156673358172312";
const DISCORD_ACTIVITY_NAME: &str = "Kakera";
pub struct DiscordPresence {
    client: DiscordIpcClient,
}
impl DiscordPresence {
    ///connects to discord and shows the currently launched vn
    pub fn start_for_vn(
        vn: &VisualNovel,
        started_at: DateTime<Utc>,
        settings: AppSettings,
    ) -> Result<Self, DiscordError> {
        let mut client = DiscordIpcClient::new(DISCORD_APP_ID);
        client.connect()?;
        let timestamps = activity::Timestamps::new().start(started_at.timestamp_millis());
        let mut assets = activity::Assets::new().large_text(vn.title.clone());
        let status_text = if settings.discord_show_active_route {
            match vn.active_route.clone() {
                Some(route_name) => format!("Reading the {route_name} route."),
                None => settings.discord_status_text.clone(),
            }
        } else {
            settings.discord_status_text.clone()
        };
        let cover_image = if !settings.discord_custom_cover_url.is_empty() {
            Some(settings.discord_custom_cover_url.clone())
        } else {
            vn.cover_url.clone()
        };
        if let Some(cover_url) = cover_image {
            assets = assets.large_image(cover_url);
        }
        let activity = activity::Activity::new()
            .name(DISCORD_ACTIVITY_NAME)
            .details(format!("Playing {}", vn.title))
            .state(status_text)
            .timestamps(timestamps)
            .assets(assets)
            .activity_type(activity::ActivityType::Playing);
        client.set_activity(activity)?;
        match client.recv() {
            Ok((_opcode, response)) => {
                println!("Discord Rich Presence response: {response}");
            }
            Err(error) => {
                println!("Could not read Discord Rich Presence response: {error}");
            }
        }
        Ok(Self { client })
    }
    ///clears the rp
    pub fn clear(mut self) -> Result<(), DiscordError> {
        self.client.clear_activity()?;
        self.client.close()?;
        Ok(())
    }
}
