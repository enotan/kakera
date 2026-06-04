use serde::{Deserialize, Serialize};
use std::cmp::PartialEq;

///the way kakera should launch a vn
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LaunchMode {
    Native,
    Wine,
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
    pub wine_prefix: Option<String>,

    #[serde(default)]
    pub wine_locale: Option<String>,

    #[serde(default)]
    pub launch_arguments: String,

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

///settings set in the settings panel

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    #[serde(default)]
    pub discord_rich_presence_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            discord_rich_presence_enabled: true,
        }
    }
}