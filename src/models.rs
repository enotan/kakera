use serde::{Deserialize, Serialize};
use std::cmp::PartialEq;

///the way kakera should launch a vn
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LaunchMode {
    Native,
    Wine,
    Proton,
}

///A visual novel stored in the library
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualNovel {
    pub id: u64,
    pub title: String,
    pub cover_url: Option<String>,
    pub description: Option<String>,

    #[serde(default)]
    pub cover_path: Option<String>,

    #[serde(default)]
    pub executable_path: Option<String>,

    #[serde(default)]
    pub launch_mode: LaunchMode,

    #[serde(default)]
    pub wine_binary: Option<String>,

    #[serde(default)]
    pub wine_prefix: Option<String>,

    #[serde(default)]
    pub wine_locale: Option<String>,

    #[serde(default)]
    pub proton_path: Option<String>,

    #[serde(default = "default_umu_game_id")]
    pub umu_game_id: String,

    #[serde(default)]
    pub launch_arguments: String,

    #[serde(default)]
    pub launch_environment: String,

    pub notes: String,
    pub routes: Vec<StoryRoute>,

    #[serde(default)]
    pub active_route: Option<String>,

    pub play_sessions: Vec<PlaySession>,
}

impl Default for LaunchMode {
    fn default() -> Self {
        LaunchMode::Native
    }
}

pub fn default_umu_game_id() -> String {
    "umu-default".to_string()
}

///A vn route
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoryRoute {
    pub name: String,
    pub completed: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlaySession {
    #[serde(alias = "visual_novel_id")]
    pub vn_id: u64,
    pub started_at: String,
    pub duration_seconds: u64,
    pub notes: Option<String>,
}

///types of notificiations
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppNotification {
    pub level: NotificationLevel,
    pub message: String,
}

///settings set in the settings panel

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    #[serde(default)]
    pub discord_rich_presence_enabled: bool,

    #[serde(default = "default_discord_status_text")]
    pub discord_status_text: String,

    #[serde(default)]
    pub discord_show_active_route: bool,

    #[serde(default)]
    pub discord_custom_cover_url: String,
}

fn default_discord_status_text() -> String {
    "Reading".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            discord_rich_presence_enabled: true,
            discord_status_text: default_discord_status_text(),
            discord_show_active_route: true,
            discord_custom_cover_url: String::new(),
        }
    }
}
