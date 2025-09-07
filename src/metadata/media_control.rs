use crate::models::{MediaSessionInfo, MetadataOptions};
use anyhow::Result;
use std::collections::HashMap;
use std::mem;
use std::ptr::null_mut;
use winapi::{
    shared::ntdef::HANDLE,
    um::{
        handleapi::CloseHandle,
        tlhelp32::{
            CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32,
            TH32CS_SNAPPROCESS,
        },
    },
};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSession
};

pub struct MediaControlCollector;

impl MediaControlCollector {
    pub fn new() -> Self {
        Self
    }

    pub async fn get_media_sessions_for_process(
        &self,
        pid: u32,
        options: &MetadataOptions,
    ) -> Result<Vec<MediaSessionInfo>> {
        let mut sessions = Vec::new();

        // Récupérer le gestionnaire de sessions média
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?;
        let manager = manager.await?;

        // Récupérer toutes les sessions actives
        let active_sessions = manager.GetSessions()?;

        // Collecter toutes les sessions dans un Vec pour éviter les problèmes Send
        let sessions_vec: Vec<_> = active_sessions.into_iter().collect();

        for session in sessions_vec {
            // Vérifier si cette session correspond au processus cible
            if self.session_matches_process(&session, pid, options) {
                let session_info = self.extract_raw_session_info(&session, pid).await?;
                sessions.push(session_info);
            }
        }

        Ok(sessions)
    }

    pub async fn get_all_raw_media_properties(
        &self,
        pid: u32,
        options: &MetadataOptions,
    ) -> Result<HashMap<String, serde_json::Value>> {
        let mut raw_data = HashMap::new();

        // Récupérer le gestionnaire de sessions média
        if let Ok(manager) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
            if let Ok(manager) = manager.await {
                // Récupérer toutes les sessions actives
                if let Ok(active_sessions) = manager.GetSessions() {
                    // Collecter toutes les sessions dans un Vec pour éviter les problèmes Send
                    let sessions_vec: Vec<_> = active_sessions.into_iter().collect();

                    for session in sessions_vec {
                        // Vérifier si cette session correspond au processus cible
                        if self.session_matches_process(&session, pid, options) {
                            // Récupérer TOUTES les propriétés brutes
                            let session_data = self.extract_all_raw_properties(&session).await?;
                            raw_data.insert("media_control_session".to_string(), session_data);
                            break; // On prend la première session qui correspond
                        }
                    }
                }
            }
        }

        Ok(raw_data)
    }

    fn session_matches_process(
        &self,
        session: &GlobalSystemMediaTransportControlsSession,
        target_pid: u32,
        options: &MetadataOptions,
    ) -> bool {
        // Si on a un nom de processus spécifique, l'utiliser directement
        let process_name = if let Some(ref name) = options.media_control_by_name {
            name.clone()
        } else {
            // Sinon, récupérer le nom du processus pour le PID cible
            self.get_process_name_by_pid(target_pid)
        };

        // Récupérer l'AppUserModelId de la session
        if let Ok(source_id) = session.SourceAppUserModelId() {
            let source_str = source_id.to_string().to_lowercase();

            // Correspondances basées sur le nom du processus
            match process_name.as_str() {
                "applemusic.exe" => source_str.contains("apple") && source_str.contains("music"),
                "spotify.exe" => source_str.contains("spotify"),
                "discord.exe" => source_str.contains("discord"),
                "firefox.exe" => source_str.contains("mozilla") || source_str.contains("firefox"),
                "chrome.exe" => source_str.contains("chrome") || source_str.contains("google"),
                "msedge.exe" => source_str.contains("edge") || source_str.contains("microsoft"),
                "brave.exe" => source_str.contains("brave"),
                "vlc.exe" => source_str.contains("vlc"),
                "winamp.exe" => source_str.contains("winamp"),
                "foobar2000.exe" => source_str.contains("foobar"),
                _ => {
                    // Pour les autres processus, on essaie de faire des correspondances génériques
                    source_str.contains("music")
                        || source_str.contains("media")
                        || source_str.contains("player")
                        || source_str.contains("browser")
                }
            }
        } else {
            false
        }
    }

    async fn extract_raw_session_info(
        &self,
        session: &GlobalSystemMediaTransportControlsSession,
        pid: u32,
    ) -> Result<MediaSessionInfo> {
        // Récupérer TOUTES les propriétés brutes sans parsing

        // Informations de base de la session
        let source_app_user_model_id = session.SourceAppUserModelId().ok().map(|s| s.to_string());
        let app_user_model_id = session.SourceAppUserModelId().ok().map(|s| s.to_string());

        // Récupérer les propriétés média BRUTES
        let media_properties = session.TryGetMediaPropertiesAsync().ok();
        let playback_info = session.GetPlaybackInfo().ok();
        let _timeline_properties = session.GetTimelineProperties().ok();

        let mut session_info = MediaSessionInfo {
            session_id: format!("session_{}", pid),
            source_app_user_model_id,
            app_user_model_id,
            media_type: None,
            playback_status: None,
            title: None,
            artist: None,
            album: None,
        };

        // Extraire les propriétés média BRUTES
        if let Some(media_props) = media_properties {
            if let Ok(props) = media_props.await {
                // Récupérer TOUTES les propriétés disponibles en BRUT
                session_info.title = props.Title().ok().map(|s| s.to_string());
                session_info.artist = props.Artist().ok().map(|s| s.to_string());
                session_info.album = props.AlbumTitle().ok().map(|s| s.to_string());
                session_info.media_type = Some(format!("{:?}", props.PlaybackType()));

                // TODO: Ajouter toutes les autres propriétés brutes disponibles
                // props.AlbumArtist(), props.TrackNumber(), props.AlbumTrackCount(), etc.
            }
        }

        // Extraire les informations de lecture BRUTES
        if let Some(playback) = playback_info {
            session_info.playback_status = Some(format!("{:?}", playback.PlaybackStatus()));
        }

        Ok(session_info)
    }

    async fn extract_all_raw_properties(
        &self,
        session: &GlobalSystemMediaTransportControlsSession,
    ) -> Result<serde_json::Value> {
        use serde_json::json;

        // Fonctions utilitaires pour extraire les propriétés
        fn try_get_string_property(value: Result<String, anyhow::Error>) -> serde_json::Value {
            match value {
                Ok(v) => json!(v),
                Err(_) => json!(null),
            }
        }

        fn try_get_numeric_property(value: Result<u32, anyhow::Error>) -> serde_json::Value {
            match value {
                Ok(v) => json!(v),
                Err(_) => json!(null),
            }
        }

        fn try_get_debug_property<T>(value: Result<T, anyhow::Error>) -> serde_json::Value
        where
            T: std::fmt::Debug,
        {
            match value {
                Ok(v) => json!(format!("{:?}", v)),
                Err(_) => json!(null),
            }
        }

        // Récupérer les informations de session
        let source_app_user_model_id = session.SourceAppUserModelId()
            .map(|s| s.to_string())
            .map_err(|e| anyhow::anyhow!("{:?}", e));

        // Récupérer les propriétés média
        let media_properties = session.TryGetMediaPropertiesAsync().ok();
        let playback_info = session.GetPlaybackInfo().ok();
        let timeline_properties = session.GetTimelineProperties().ok();

        let mut media_props_json = json!({
            "title": json!(null),
            "artist": json!(null),
            "album_title": json!(null),
            "album_artist": json!(null),
            "track_number": json!(null),
            "album_track_count": json!(null),
            "playback_type": json!(null),
            "subtitle": json!(null),
            "genres": json!(null),
        });

        // Extraire les vraies propriétés média
        if let Some(media_props) = media_properties {
            if let Ok(props) = media_props.await {
                media_props_json["title"] = try_get_string_property(props.Title().map(|s| s.to_string()).map_err(|e| anyhow::anyhow!("{:?}", e)));
                media_props_json["artist"] = try_get_string_property(props.Artist().map(|s| s.to_string()).map_err(|e| anyhow::anyhow!("{:?}", e)));
                media_props_json["album_title"] = try_get_string_property(props.AlbumTitle().map(|s| s.to_string()).map_err(|e| anyhow::anyhow!("{:?}", e)));
                media_props_json["album_artist"] = try_get_string_property(props.AlbumArtist().map(|s| s.to_string()).map_err(|e| anyhow::anyhow!("{:?}", e)));
                media_props_json["track_number"] = try_get_numeric_property(props.TrackNumber().map(|v| v as u32).map_err(|e| anyhow::anyhow!("{:?}", e)));
                media_props_json["album_track_count"] = try_get_numeric_property(props.AlbumTrackCount().map(|v| v as u32).map_err(|e| anyhow::anyhow!("{:?}", e)));
                media_props_json["playback_type"] = try_get_debug_property(Ok(props.PlaybackType()));
                media_props_json["subtitle"] = try_get_string_property(props.Subtitle().map(|s| s.to_string()).map_err(|e| anyhow::anyhow!("{:?}", e)));
                media_props_json["genres"] = try_get_debug_property(Ok(props.Genres()));
            }
        }

        let mut playback_info_json = json!({
            "playback_status": json!(null),
            "playback_type": json!(null),
            "auto_repeat_mode": json!(null),
            "playback_rate": json!(null),
            "is_shuffle_active": json!(null),
        });

        // Extraire les vraies informations de lecture
        if let Some(playback) = playback_info {
            playback_info_json["playback_status"] = try_get_debug_property(Ok(playback.PlaybackStatus()));
            playback_info_json["playback_type"] = try_get_debug_property(Ok(playback.PlaybackType()));
            playback_info_json["auto_repeat_mode"] = try_get_debug_property(Ok(playback.AutoRepeatMode()));
            playback_info_json["playback_rate"] = try_get_debug_property(Ok(playback.PlaybackRate()));
            playback_info_json["is_shuffle_active"] = try_get_debug_property(Ok(playback.IsShuffleActive()));
        }

        let mut timeline_props_json = json!({
            "start_time": json!(null),
            "end_time": json!(null),
            "position": json!(null),
            "min_seek_time": json!(null),
            "max_seek_time": json!(null),
        });

        // Extraire les vraies propriétés de timeline
        if let Some(timeline) = timeline_properties {
            timeline_props_json["start_time"] = try_get_debug_property(Ok(timeline.StartTime()));
            timeline_props_json["end_time"] = try_get_debug_property(Ok(timeline.EndTime()));
            timeline_props_json["position"] = try_get_debug_property(Ok(timeline.Position()));
            timeline_props_json["min_seek_time"] = try_get_debug_property(Ok(timeline.MinSeekTime()));
            timeline_props_json["max_seek_time"] = try_get_debug_property(Ok(timeline.MaxSeekTime()));
        }

        // Construire l'objet final avec toutes les propriétés réelles
        let source_app_id_result = source_app_user_model_id;
        let all_properties = json!({
            "session_info": {
                "source_app_user_model_id": try_get_string_property(source_app_id_result),
                "app_user_model_id": try_get_string_property(session.SourceAppUserModelId().map(|s| s.to_string()).map_err(|e| anyhow::anyhow!("{:?}", e))),
            },
            "media_properties": media_props_json,
            "playback_info": playback_info_json,
            "timeline_properties": timeline_props_json,
        });

        Ok(all_properties)
    }

    fn get_process_name_by_pid(&self, pid: u32) -> String {
        unsafe {
            let snapshot: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot != null_mut() {
                let mut pe32: PROCESSENTRY32 = mem::zeroed();
                pe32.dwSize = mem::size_of::<PROCESSENTRY32>() as u32;

                if Process32First(snapshot, &mut pe32) != 0 {
                    loop {
                        if pe32.th32ProcessID == pid {
                            let name = self.c_string_to_string_260(&pe32.szExeFile);
                            CloseHandle(snapshot);
                            return name.to_lowercase();
                        }

                        if Process32Next(snapshot, &mut pe32) == 0 {
                            break;
                        }
                    }
                }
                CloseHandle(snapshot);
            }
        }
        String::new()
    }

    fn c_string_to_string_260(&self, c_str: &[i8; 260]) -> String {
        let end = c_str.iter().position(|&x| x == 0).unwrap_or(c_str.len());
        let bytes: Vec<u8> = c_str[..end].iter().map(|&x| x as u8).collect();
        String::from_utf8_lossy(&bytes).to_string()
    }
}
