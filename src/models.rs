use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataOptions {
    // Métadonnées de base (toujours utiles)
    pub basic_info: bool,           // PID, nom, chemin, titre fenêtre
    pub memory_info: bool,          // Utilisation mémoire
    pub window_info: bool,          // Fenêtres du processus
    
    // Métadonnées détaillées (optionnelles)
    pub cpu_info: bool,             // Temps CPU (gourmand)
    pub thread_info: bool,          // Threads (gourmand)
    pub module_info: bool,          // Modules chargés (gourmand)
    pub handle_info: bool,          // Handles (gourmand)
    pub environment_vars: bool,     // Variables d'environnement (gourmand)
    
    // Media Control
    pub media_control: bool,        // Sessions média
    pub media_control_by_name: Option<String>, // Nom du processus pour Media Control
}

impl Default for MetadataOptions {
    fn default() -> Self {
        Self {
            // Par défaut : seulement les infos utiles
            basic_info: true,
            memory_info: true,
            window_info: true,
            
            // Par défaut : désactivé (gourmand)
            cpu_info: false,
            thread_info: false,
            module_info: false,
            handle_info: false,
            environment_vars: false,
            
            // Media Control
            media_control: true,
            media_control_by_name: None,
        }
    }
}

// Structure simple pour le scan des applications (comme avant)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub window_title: Option<String>,
    pub executable_path: Option<String>,
    pub subprocesses: Vec<ProcessInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationInfo {
    pub main_process: ProcessInfo,
    pub total_processes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub applications: Vec<ApplicationInfo>,
    pub scan_timestamp: String,
    pub total_applications: usize,
}

// Structure dynamique pour récupérer TOUTES les métadonnées possibles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetadata {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    
    // Informations de base
    pub executable_path: Option<String>,
    pub command_line: Option<String>,
    pub working_directory: Option<String>,
    pub window_title: Option<String>,
    
    // Informations temporelles
    pub creation_time: Option<String>,
    pub exit_time: Option<String>,
    
    // Informations mémoire
    pub memory_info: Option<MemoryInfo>,
    
    // Informations CPU
    pub cpu_info: Option<CpuInfo>,
    
    // Informations système
    pub thread_count: u32,
    pub priority_class: Option<String>,
    pub handle_count: u32,
    pub page_fault_count: u32,
    pub peak_working_set_size: u64,
    pub working_set_size: u64,
    pub quota_peak_paged_pool_usage: u64,
    pub quota_paged_pool_usage: u64,
    pub quota_peak_non_paged_pool_usage: u64,
    pub quota_non_paged_pool_usage: u64,
    pub pagefile_usage: u64,
    pub peak_pagefile_usage: u64,
    
    // NOUVELLES INFORMATIONS DYNAMIQUES
    pub windows: Vec<WindowInfo>,
    pub threads: Vec<ThreadInfo>,
    pub modules: Vec<ModuleInfo>,
    pub media_sessions: Vec<MediaSessionInfo>,
    pub handles: Vec<HandleInfo>,
    pub environment_variables: std::collections::HashMap<String, String>,
    pub raw_data: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub hwnd: u64,
    pub class_name: String,
    pub window_title: String,
    pub process_id: u32,
    pub thread_id: u32,
    pub is_visible: bool,
    pub window_rect: Option<WindowRect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadInfo {
    pub thread_id: u32,
    pub process_id: u32,
    pub creation_time: Option<String>,
    pub exit_time: Option<String>,
    pub kernel_time: u64,
    pub user_time: u64,
    pub priority: i32,
    pub base_priority: i32,
    pub context_switches: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub module_name: String,
    pub module_path: String,
    pub base_address: u64,
    pub module_size: u32,
    pub entry_point: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaSessionInfo {
    pub session_id: String,
    pub source_app_user_model_id: Option<String>,
    pub app_user_model_id: Option<String>,
    pub media_type: Option<String>,
    pub playback_status: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandleInfo {
    pub handle_type: String,
    pub handle_value: u64,
    pub object_name: Option<String>,
    pub access_mask: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub working_set_size: u64,
    pub peak_working_set_size: u64,
    pub pagefile_usage: u64,
    pub peak_pagefile_usage: u64,
    pub private_usage: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub kernel_time: u64,
    pub user_time: u64,
    pub creation_time: u64,
    pub exit_time: u64,
}
