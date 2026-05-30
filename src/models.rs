use serde::{Deserialize, Serialize};
use std::cmp::PartialEq;

///A visual novel stored in the library
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualNovel {
    pub id: u64,
    pub title: String,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub notes: String,
    pub routes: Vec<StoryRoute>,
    pub play_sessions: Vec<PlaySession>,
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
    pub visual_novel_id: u64,
    pub started_at: String,
    pub duration_seconds: u64,
    pub notes: Option<String>,
}
