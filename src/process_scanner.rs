use crate::models::{ApplicationInfo, ProcessInfo, ScanResult, ProcessMetadata, MetadataOptions};
use anyhow::Result;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use std::ptr::null_mut;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::mem;
use winapi::{
    shared::{
        minwindef::{BOOL, DWORD, LPARAM, MAX_PATH},
        windef::HWND,
        ntdef::HANDLE,
    },
    um::{
        handleapi::CloseHandle,
        tlhelp32::{
            CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32,
            TH32CS_SNAPPROCESS,
        },
        winuser::{EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible},
    },
};

pub struct ProcessScanner;

impl ProcessScanner {
    pub fn new() -> Self {
        Self
    }

    // Fonction principale pour scanner les applications (comme avant)
    pub fn scan_applications(&self) -> Result<ScanResult> {
        let processes = self.get_all_processes()?;
        let window_processes = self.get_processes_with_windows()?;
        
        // Grouper les processus par application principale
        let applications = self.group_processes_by_application(processes, window_processes);
        
        let total_applications = applications.len();
        let scan_timestamp = self.get_current_timestamp();
        
        Ok(ScanResult {
            applications,
            scan_timestamp,
            total_applications,
        })
    }

    // NOUVELLE FONCTION : Récupérer toutes les métadonnées brutes d'un PID spécifique
    pub fn get_process_metadata(&self, pid: u32, options: Option<MetadataOptions>) -> Result<ProcessMetadata> {
        use crate::{ProcessMetadataCollector, MediaControlCollector};
        
        // Utiliser les options par défaut si aucune n'est fournie
        let options = options.unwrap_or_default();
        
        // Récupérer les métadonnées de base du processus
        let mut metadata = ProcessMetadataCollector::new().collect_all_metadata(pid, &options)?;
        
        // Ajouter les sessions média si demandé (approche hybride : seulement Media Control en async)
        if options.media_control {
            let media_collector = MediaControlCollector::new();
            
            // Utiliser spawn_blocking SEULEMENT pour Media Control (problème Send trait)
            let (media_sessions, raw_media_data) = std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let sessions = rt.block_on(media_collector.get_media_sessions_for_process(pid, &options))?;
                let raw_data = rt.block_on(media_collector.get_all_raw_media_properties(pid, &options))?;
                Ok::<(Vec<_>, HashMap<String, serde_json::Value>), anyhow::Error>((sessions, raw_data))
            }).join().unwrap()?;
            
            metadata.media_sessions = media_sessions;
            metadata.raw_data.extend(raw_media_data);
        }
        
        Ok(metadata)
    }

    // NOUVELLE FONCTION : Détecter l'onglet actif d'un navigateur
    pub fn get_active_browser_tab(&self, pid: u32) -> Result<Option<crate::models::WindowInfo>> {
        use crate::ProcessMetadataCollector;
        let collector = ProcessMetadataCollector::new();
        collector.get_active_browser_tab(pid)
    }

    // NOUVELLE FONCTION : Trouver un PID par nom d'exécutable
    pub fn find_pid_by_executable_name(&self, executable_name: &str) -> Result<Option<u32>> {
        let snapshot = unsafe { 
            winapi::um::tlhelp32::CreateToolhelp32Snapshot(
                winapi::um::tlhelp32::TH32CS_SNAPPROCESS, 
                0
            )
        };

        if snapshot == winapi::um::handleapi::INVALID_HANDLE_VALUE {
            return Err(anyhow::anyhow!("Impossible de créer le snapshot des processus"));
        }

        let mut process_entry = winapi::um::tlhelp32::PROCESSENTRY32 {
            dwSize: std::mem::size_of::<winapi::um::tlhelp32::PROCESSENTRY32>() as u32,
            cntUsage: 0,
            th32ProcessID: 0,
            th32DefaultHeapID: 0,
            th32ModuleID: 0,
            cntThreads: 0,
            th32ParentProcessID: 0,
            pcPriClassBase: 0,
            dwFlags: 0,
            szExeFile: [0; 260],
        };

        let mut found_pid = None;

        unsafe {
            if winapi::um::tlhelp32::Process32First(snapshot, &mut process_entry) != 0 {
                loop {
                    let process_name = std::ffi::CStr::from_ptr(process_entry.szExeFile.as_ptr())
                        .to_string_lossy()
                        .to_lowercase();
                    
                    if process_name == executable_name.to_lowercase() {
                        found_pid = Some(process_entry.th32ProcessID);
                        break;
                    }
                    
                    if winapi::um::tlhelp32::Process32Next(snapshot, &mut process_entry) == 0 {
                        break;
                    }
                }
            }
            
            let _ = winapi::um::handleapi::CloseHandle(snapshot);
        }

        Ok(found_pid)
    }

    // NOUVELLE FONCTION : Surveiller un processus par nom d'exécutable
    pub fn monitor_process_by_name(&self, executable_name: &str, options: Option<crate::models::MetadataOptions>) -> Result<Option<crate::models::ProcessMetadata>> {
        if let Some(pid) = self.find_pid_by_executable_name(executable_name)? {
            // Le processus existe, récupérer ses métadonnées
            Ok(Some(self.get_process_metadata(pid, options)?))
        } else {
            // Le processus n'existe pas
            Ok(None)
        }
    }


    // Fonctions pour le scan des applications (comme avant)
    fn get_all_processes(&self) -> Result<Vec<ProcessInfo>> {
        let mut processes = Vec::new();
        
        unsafe {
            let snapshot: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot != null_mut() {
                let mut pe32: PROCESSENTRY32 = mem::zeroed();
                pe32.dwSize = mem::size_of::<PROCESSENTRY32>() as u32;
                
                if Process32First(snapshot, &mut pe32) != 0 {
                    loop {
                        let process_info = ProcessInfo {
                            pid: pe32.th32ProcessID,
                            name: self.c_string_to_string(&pe32.szExeFile),
                            window_title: None,
                            executable_path: None,
                            subprocesses: Vec::new(),
                        };
                        processes.push(process_info);

                        if Process32Next(snapshot, &mut pe32) == 0 {
                            break;
                        }
                    }
                }
                CloseHandle(snapshot);
            }
        }

        Ok(processes)
    }

    fn get_subprocesses(&self, parent_pid: u32) -> Vec<ProcessInfo> {
        let mut subprocesses = Vec::new();
        
        unsafe {
            let snapshot: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot != null_mut() {
                let mut pe32: PROCESSENTRY32 = mem::zeroed();
                pe32.dwSize = mem::size_of::<PROCESSENTRY32>() as u32;
                
                if Process32First(snapshot, &mut pe32) != 0 {
                    loop {
                        if pe32.th32ParentProcessID == parent_pid {
                            let subprocess_info = ProcessInfo {
                                pid: pe32.th32ProcessID,
                                name: self.c_string_to_string(&pe32.szExeFile),
                                window_title: None,
                                executable_path: None,
                                subprocesses: Vec::new(),
                            };
                            subprocesses.push(subprocess_info);
                        }

                        if Process32Next(snapshot, &mut pe32) == 0 {
                            break;
                        }
                    }
                }
                CloseHandle(snapshot);
            }
        }

        subprocesses
    }

    fn get_processes_with_windows(&self) -> Result<HashMap<u32, String>> {
        let mut window_processes = HashMap::new();
        
        unsafe {
            EnumWindows(
                Some(enum_windows_proc),
                &mut window_processes as *mut _ as LPARAM,
            );
        }

        Ok(window_processes)
    }

    fn group_processes_by_application(
        &self,
        mut processes: Vec<ProcessInfo>,
        window_processes: HashMap<u32, String>,
    ) -> Vec<ApplicationInfo> {
        // Mettre à jour les processus avec les titres de fenêtre
        for process in &mut processes {
            if let Some(title) = window_processes.get(&process.pid) {
                process.window_title = Some(title.clone());
            }
        }

        // Filtrer pour ne garder que les applications (processus avec fenêtres visibles)
        let applications: Vec<ProcessInfo> = processes
            .into_iter()
            .filter(|p| p.window_title.is_some())
            .collect();

        // Grouper par nom d'application
        let mut grouped: HashMap<String, Vec<ProcessInfo>> = HashMap::new();
        for app in applications {
            grouped.entry(app.name.clone()).or_insert_with(Vec::new).push(app);
        }

        // Créer les ApplicationInfo avec détection des sous-processus
        grouped
            .into_iter()
            .map(|(_, mut processes)| {
                // Trier par PID pour avoir le processus principal en premier
                processes.sort_by_key(|p| p.pid);
                
                let main_process = processes.remove(0);
                
                // Récupérer les sous-processus pour le processus principal
                let subprocesses = self.get_subprocesses(main_process.pid);
                
                // Ajouter les autres processus du même nom comme sous-processus
                let mut all_subprocesses = subprocesses;
                for process in processes {
                    all_subprocesses.push(process);
                }
                
                let total_processes = 1 + all_subprocesses.len();

                ApplicationInfo {
                    main_process: ProcessInfo {
                        subprocesses: all_subprocesses,
                        ..main_process
                    },
                    total_processes,
                }
            })
            .collect()
    }

    fn c_string_to_string(&self, c_str: &[i8; 260]) -> String {
        let end = c_str.iter().position(|&x| x == 0).unwrap_or(c_str.len());
        let bytes: Vec<u8> = c_str[..end].iter().map(|&x| x as u8).collect();
        String::from_utf8_lossy(&bytes).to_string()
    }

    fn get_current_timestamp(&self) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        format!("{}", now)
    }
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let window_processes = &mut *(lparam as *mut HashMap<u32, String>);
    
    if IsWindowVisible(hwnd) != 0 {
        let mut window_text: [u16; MAX_PATH] = [0; MAX_PATH];
        let text_len = GetWindowTextW(hwnd, window_text.as_mut_ptr(), MAX_PATH as i32);
        
        if text_len > 0 {
            let os_string = OsString::from_wide(&window_text[..text_len as usize]);
            if let Ok(title) = os_string.into_string() {
                if !title.is_empty() {
                    let mut process_id: DWORD = 0;
                    GetWindowThreadProcessId(hwnd, &mut process_id);
                    
                    if process_id != 0 {
                        window_processes.insert(process_id, title);
                    }
                }
            }
        }
    }
    
    1 // Continue enumeration
}