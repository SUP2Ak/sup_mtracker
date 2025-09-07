pub mod process_scanner;
pub mod models;
pub mod metadata;
pub mod realtime_monitor;

pub use process_scanner::ProcessScanner;
pub use metadata::{ProcessMetadataCollector, MediaControlCollector};
pub use realtime_monitor::{RealtimeProcessMonitor, MonitorConfig, ProcessMonitorState, create_simple_monitor};

// Réexporter SEULEMENT les types publics nécessaires
pub use models::{
    MetadataOptions,
    ProcessInfo,
    ApplicationInfo,
    ScanResult,
    ProcessMetadata,
    WindowInfo,
    WindowRect,
    ThreadInfo,
    ModuleInfo,
    MediaSessionInfo,
    HandleInfo,
    MemoryInfo,
    CpuInfo,
};