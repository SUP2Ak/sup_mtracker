use crate::{
    models::{MetadataOptions, ProcessMetadata, WindowInfo},
    ProcessScanner,
};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sup_common::{debug_eprintln, debug_println};
use tokio::time::interval;

/// Configuration pour la surveillance en temps réel
pub struct MonitorConfig {
    /// Nom de l'exécutable à surveiller
    pub executable_name: String,
    /// Intervalle de vérification (en secondes)
    pub check_interval: u64,
    /// Options de métadonnées à collecter
    pub metadata_options: MetadataOptions,
    /// Callback appelé quand les données changent
    pub on_data_change: Option<Box<dyn Fn(&ProcessMetadata) + Send + Sync>>,
}

impl Clone for MonitorConfig {
    fn clone(&self) -> Self {
        Self {
            executable_name: self.executable_name.clone(),
            check_interval: self.check_interval,
            metadata_options: self.metadata_options.clone(),
            on_data_change: None, // On ne peut pas cloner le callback
        }
    }
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            executable_name: String::new(),
            check_interval: 3, // 3 secondes par défaut
            metadata_options: MetadataOptions::default(),
            on_data_change: None,
        }
    }
}

/// État de surveillance d'un processus
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessMonitorState {
    /// Dernières métadonnées collectées
    pub last_metadata: Option<ProcessMetadata>,
    /// Dernier onglet actif détecté
    pub last_active_tab: Option<WindowInfo>,
    /// Timestamp de la dernière mise à jour (en millisecondes depuis l'epoch)
    #[serde(skip_serializing, skip_deserializing)]
    pub last_update: Option<Instant>,
    /// Le processus est-il actuellement actif ?
    pub is_active: bool,
}

impl Default for ProcessMonitorState {
    fn default() -> Self {
        Self {
            last_metadata: None,
            last_active_tab: None,
            last_update: None,
            is_active: false,
        }
    }
}

/// Moniteur de processus en temps réel
pub struct RealtimeProcessMonitor {
    config: MonitorConfig,
    state: Arc<Mutex<ProcessMonitorState>>,
    is_running: Arc<Mutex<bool>>,
}

impl RealtimeProcessMonitor {
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(ProcessMonitorState::default())),
            is_running: Arc::new(Mutex::new(false)),
        }
    }

    /// Démarrer la surveillance
    pub async fn start(&self) -> Result<()> {
        let mut is_running = self.is_running.lock().unwrap();
        if *is_running {
            return Ok(()); // Déjà en cours
        }
        *is_running = true;
        drop(is_running);

        let config = self.config.clone();
        let state = Arc::clone(&self.state);
        let is_running = Arc::clone(&self.is_running);

        // Démarrer la boucle de surveillance
        tokio::task::spawn_blocking(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let mut interval = interval(Duration::from_secs(config.check_interval));

                loop {
                    interval.tick().await;

                    // Vérifier si on doit continuer
                    {
                        let running = is_running.lock().unwrap();
                        if !*running {
                            break;
                        }
                    }

                    // Vérifier le processus avec timeout pour éviter les blocages
                    let check_result = tokio::time::timeout(
                        Duration::from_secs(5), // Timeout de 5 secondes
                        Self::check_process(&config, &state)
                    ).await;

        match check_result {
            Ok(Ok(has_changes)) => {
                if has_changes {
                    debug_println!(
                        "🔄 Changements détectés pour {}",
                        config.executable_name
                    );
                }
            }
            Ok(Err(e)) => {
                debug_eprintln!("❌ Erreur lors de la vérification: {}", e);
            }
            Err(_) => {
                debug_eprintln!("⏰ Timeout lors de la vérification de {}", config.executable_name);
            }
        }
                }
            })
        });

        Ok(())
    }

    /// Arrêter la surveillance
    pub fn stop(&self) {
        let mut is_running = self.is_running.lock().unwrap();
        *is_running = false;
    }

    /// Obtenir l'état actuel
    pub fn get_state(&self) -> ProcessMonitorState {
        match self.state.try_lock() {
            Ok(guard) => guard.clone(),
            Err(_) => {
                debug_println!(
                    "⚠️ Impossible d'accéder au state (lock occupé), retour d'un state vide"
                );
                ProcessMonitorState::default()
            }
        }
    }

    /// Vérifier le processus et détecter les changements
    async fn check_process(
        config: &MonitorConfig,
        state: &Arc<Mutex<ProcessMonitorState>>,
    ) -> Result<bool> {
        let mut has_changes = false;

        debug_println!("🔍 Vérification du processus {}...", config.executable_name);

        // Vérifier si le processus existe (approche synchrone)
        let executable_name = config.executable_name.clone();
        let options = config.metadata_options.clone();
        let metadata_result = tokio::task::spawn_blocking(move || {
            let scanner = ProcessScanner::new();
            scanner.monitor_process_by_name(&executable_name, Some(options))
        }).await;

        if let Ok(Ok(Some(metadata))) = metadata_result {
            debug_println!(
                "✅ Processus {} trouvé, PID: {}",
                config.executable_name,
                metadata.pid
            );
            // Le processus existe
            let mut current_state = match state.try_lock() {
                Ok(guard) => guard,
                Err(_) => {
                    debug_println!(
                        "⚠️ Impossible d'accéder au state (lock occupé), skip cette vérification"
                    );
                    return Ok(false);
                }
            };

            // Vérifier si c'est un nouveau processus ou si les données ont changé
            let is_new_process = current_state
                .last_metadata
                .as_ref()
                .map(|last| last.pid != metadata.pid)
                .unwrap_or(true);

            let metadata_changed = current_state
                .last_metadata
                .as_ref()
                .map(|last| Self::has_metadata_changed(last, &metadata))
                .unwrap_or(true);

            if is_new_process || metadata_changed {
                current_state.last_metadata = Some(metadata.clone());
                current_state.is_active = true;
                current_state.last_update = Some(Instant::now());
                has_changes = true;

                // Appeler le callback si défini
                if let Some(callback) = &config.on_data_change {
                    callback(&metadata);
                }
            }

            // Vérifier l'onglet actif pour les navigateurs
            if config.metadata_options.window_info {
                let scanner = ProcessScanner::new();
                if let Ok(Some(active_tab)) = scanner.get_active_browser_tab(metadata.pid) {
                    let tab_changed = current_state
                        .last_active_tab
                        .as_ref()
                        .map(|last| last.window_title != active_tab.window_title)
                        .unwrap_or(true);

                    if tab_changed {
                        current_state.last_active_tab = Some(active_tab.clone());
                        has_changes = true;
                        debug_println!(
                            "🔄 Nouvel onglet actif détecté: {}",
                            active_tab.window_title
                        );
                    }
                }
            }
        } else {
            // Le processus n'existe plus ou timeout
            let mut current_state = match state.try_lock() {
                Ok(guard) => guard,
                Err(_) => {
                    debug_println!("⚠️ Impossible d'accéder au state (lock occupé), skip cette vérification");
                    return Ok(false);
                }
            };
            
            if current_state.is_active {
                current_state.is_active = false;
                current_state.last_update = Some(Instant::now());
                has_changes = true;
                debug_println!("⚠️ Processus {} arrêté ou timeout", config.executable_name);
            }
        }

        Ok(has_changes)
    }

    /// Vérifier si les métadonnées ont changé
    fn has_metadata_changed(last: &ProcessMetadata, current: &ProcessMetadata) -> bool {
        // Comparer les champs importants
        if last.window_title != current.window_title
            || last.working_set_size != current.working_set_size
            || last.handle_count != current.handle_count
            || last.windows.len() != current.windows.len()
        {
            return true;
        }

        // Comparer les media sessions (important pour Apple Music, Spotify, etc.)
        if last.media_sessions.len() != current.media_sessions.len() {
            return true;
        }

        for (i, (last_session, current_session)) in last
            .media_sessions
            .iter()
            .zip(current.media_sessions.iter())
            .enumerate()
        {
            if last_session.title != current_session.title
                || last_session.artist != current_session.artist
                || last_session.album != current_session.album
            {
                debug_println!(
                    "🎵 Changement de média détecté à l'index {}: '{}' -> '{}'",
                    i,
                    last_session.title.as_ref().unwrap_or(&"None".to_string()),
                    current_session
                        .title
                        .as_ref()
                        .unwrap_or(&"None".to_string())
                );
                return true;
            }
        }

        false
    }
}

/// Fonction utilitaire pour créer un moniteur simple
pub fn create_simple_monitor(executable_name: &str, check_interval: u64) -> RealtimeProcessMonitor {
    let mut config = MonitorConfig::default();
    config.executable_name = executable_name.to_string();
    config.check_interval = check_interval;

    // Options optimisées pour la surveillance en temps réel
    config.metadata_options.basic_info = true;
    config.metadata_options.memory_info = true;
    config.metadata_options.window_info = true;
    config.metadata_options.media_control = true;

    // Désactiver les options gourmandes
    config.metadata_options.cpu_info = false;
    config.metadata_options.thread_info = false;
    config.metadata_options.module_info = false;
    config.metadata_options.handle_info = false;
    config.metadata_options.environment_vars = false;

    RealtimeProcessMonitor::new(config)
}
