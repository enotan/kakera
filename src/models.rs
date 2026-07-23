use serde::{Deserialize, Serialize};
use std::cmp::PartialEq;
///the way kakera should launch a vn
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LaunchMode {
    Native,
    Wine,
    Proton,
    Steam,
}
///A visual novel stored in the library
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualNovel {
    pub id: u64,
    pub title: String,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub is_favourite: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub cover_path: Option<String>,
    #[serde(default)]
    pub executable_path: Option<String>,
    #[serde(default)]
    pub launch_mode: LaunchMode,
    #[serde(default)]
    pub steam_app_id: Option<u32>,
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
    #[serde(default)]
    pub save_sync: SaveSyncConfig,
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
///where a vn saves its save data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SaveLocation {
    #[serde(default)]
    pub id: String,

    pub path: String,

    #[serde(default)]
    pub label: String,
}

///controls save snapshots for 1 vn
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SaveSyncConfig {
    pub enabled: bool,
    pub locations: Vec<SaveLocation>,
    pub snapshot_before_launch: bool,
    pub snapshot_after_exit: bool,
    pub backup_before_restore: bool,
}

impl Default for SaveSyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            locations: Vec::new(),
            snapshot_before_launch: true,
            snapshot_after_exit: true,
            backup_before_restore: true,
        }
    }
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
///the order to display vns in the library
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LibrarySortMode {
    Added,
    TitleAsc,
    TitleDesc,
    LastPlayed,
    MostPlaytime,
}
impl Default for LibrarySortMode {
    fn default() -> Self {
        LibrarySortMode::Added
    }
}
///the "categories" i guess? of vns that u can set in the library.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LibraryFilterMode {
    All,
    Favourites,
}
impl Default for LibraryFilterMode {
    fn default() -> Self {
        LibraryFilterMode::All
    }
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
    #[serde(default)]
    pub library_sort_mode: LibrarySortMode,
    #[serde(default)]
    pub library_filter_mode: LibraryFilterMode,
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
            library_sort_mode: LibrarySortMode::default(),
            library_filter_mode: LibraryFilterMode::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::VisualNovel;

    #[test]
    fn older_vn_uses_default_save_sync_config() {
        let old_json = r#"
        {
            "id": 1,
            "title": "Legacy",
            "cover_url": null,
            "description": null,
            "notes": "",
            "routes": [],
            "play_sessions": []
        }
        "#;

        let vn = serde_json::from_str::<VisualNovel>(old_json)
            .expect("an older VisualNovel should still deserialize");

        assert!(!vn.save_sync.enabled);
        assert!(vn.save_sync.locations.is_empty());
        assert!(vn.save_sync.snapshot_before_launch);
        assert!(vn.save_sync.snapshot_after_exit);
        assert!(vn.save_sync.backup_before_restore);
    }
}
