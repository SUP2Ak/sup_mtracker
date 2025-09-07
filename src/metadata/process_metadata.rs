use crate::models::{
    ProcessMetadata, WindowInfo, WindowRect, ThreadInfo, ModuleInfo, 
    MemoryInfo, CpuInfo, MetadataOptions
};
use anyhow::Result;
use std::collections::HashMap;
use std::ptr::null_mut;
use std::mem;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use winapi::{
    shared::{
        minwindef::{BOOL, DWORD, LPARAM, MAX_PATH, FILETIME},
        windef::{HWND, RECT},
        ntdef::HANDLE,
    },
    um::{
        handleapi::CloseHandle,
        tlhelp32::{
            CreateToolhelp32Snapshot, Thread32First, Thread32Next, THREADENTRY32,
            TH32CS_SNAPTHREAD, Module32First, Module32Next, MODULEENTRY32,
            TH32CS_SNAPMODULE, Process32First, Process32Next, PROCESSENTRY32,
            TH32CS_SNAPPROCESS,
        },
        winuser::{
            EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
            GetClassNameW, GetWindowRect,
        },
        processthreadsapi::{
            OpenProcess, GetProcessTimes, GetProcessHandleCount,
        },
        psapi::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        },
        winbase::QueryFullProcessImageNameW,
        winnt::{PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
    },
};

pub struct ProcessMetadataCollector;

impl ProcessMetadataCollector {
    pub fn new() -> Self {
        Self
    }

    pub fn collect_all_metadata(&self, pid: u32, options: &MetadataOptions) -> Result<ProcessMetadata> {
        unsafe {
            let process_handle = OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
                0,
                pid,
            );

            if process_handle == null_mut() {
                return Err(anyhow::anyhow!("Impossible d'ouvrir le processus PID: {}", pid));
            }

            let mut metadata = ProcessMetadata {
                pid,
                parent_pid: 0,
                name: String::new(),
                executable_path: None,
                command_line: None,
                working_directory: None,
                window_title: None,
                creation_time: None,
                exit_time: None,
                memory_info: None,
                cpu_info: None,
                thread_count: 0,
                priority_class: None,
                handle_count: 0,
                page_fault_count: 0,
                peak_working_set_size: 0,
                working_set_size: 0,
                quota_peak_paged_pool_usage: 0,
                quota_paged_pool_usage: 0,
                quota_peak_non_paged_pool_usage: 0,
                quota_non_paged_pool_usage: 0,
                pagefile_usage: 0,
                peak_pagefile_usage: 0,
                windows: Vec::new(),
                threads: Vec::new(),
                modules: Vec::new(),
                media_sessions: Vec::new(),
                handles: Vec::new(),
                environment_variables: HashMap::new(),
                raw_data: HashMap::new(),
            };

            // Récupérer les informations selon les options
            if options.basic_info {
                metadata.executable_path = self.get_executable_path(process_handle);
            }
            
            if options.memory_info {
                metadata.memory_info = self.get_memory_info(process_handle);
            }
            
            if options.cpu_info {
                metadata.cpu_info = self.get_cpu_info(process_handle);
            }
            
            if options.window_info {
                metadata.windows = self.get_windows_for_process(pid)?;
            }
            
            if options.thread_info {
                metadata.threads = self.get_threads_for_process(pid)?;
            }
            
            if options.module_info {
                metadata.modules = self.get_modules_for_process(pid)?;
            }
            
            if options.environment_vars {
                metadata.environment_variables = self.get_environment_variables(pid)?;
            }

            // Récupérer le nombre de handles si demandé
            if options.handle_info {
                let mut handle_count = 0u32;
                GetProcessHandleCount(process_handle, &mut handle_count);
                metadata.handle_count = handle_count;
            }

            // Récupérer les informations de mémoire détaillées si demandé
            if options.memory_info {
                if let Some(mem_info) = &metadata.memory_info {
                    metadata.page_fault_count = mem_info.working_set_size as u32;
                    metadata.peak_working_set_size = mem_info.peak_working_set_size;
                    metadata.working_set_size = mem_info.working_set_size;
                    metadata.pagefile_usage = mem_info.pagefile_usage;
                    metadata.peak_pagefile_usage = mem_info.peak_pagefile_usage;
                }
            }

            CloseHandle(process_handle);
            Ok(metadata)
        }
    }

    fn get_executable_path(&self, process_handle: HANDLE) -> Option<String> {
        unsafe {
            let mut buffer: [u16; MAX_PATH] = [0; MAX_PATH];
            let mut size = MAX_PATH as u32;
            
            if QueryFullProcessImageNameW(process_handle, 0, buffer.as_mut_ptr(), &mut size) != 0 {
                let os_string = OsString::from_wide(&buffer[..size as usize]);
                os_string.into_string().ok()
            } else {
                None
            }
        }
    }

    fn get_memory_info(&self, process_handle: HANDLE) -> Option<MemoryInfo> {
        unsafe {
            let mut pmc: PROCESS_MEMORY_COUNTERS = mem::zeroed();
            
            if GetProcessMemoryInfo(process_handle, &mut pmc, mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32) != 0 {
                Some(MemoryInfo {
                    working_set_size: pmc.WorkingSetSize as u64,
                    peak_working_set_size: pmc.PeakWorkingSetSize as u64,
                    pagefile_usage: pmc.PagefileUsage as u64,
                    peak_pagefile_usage: pmc.PeakPagefileUsage as u64,
                    private_usage: pmc.QuotaPagedPoolUsage as u64,
                })
            } else {
                None
            }
        }
    }

    fn get_cpu_info(&self, process_handle: HANDLE) -> Option<CpuInfo> {
        unsafe {
            let mut creation_time: FILETIME = mem::zeroed();
            let mut exit_time: FILETIME = mem::zeroed();
            let mut kernel_time: FILETIME = mem::zeroed();
            let mut user_time: FILETIME = mem::zeroed();
            
            if GetProcessTimes(
                process_handle,
                &mut creation_time,
                &mut exit_time,
                &mut kernel_time,
                &mut user_time,
            ) != 0 {
                Some(CpuInfo {
                    kernel_time: self.filetime_to_u64(kernel_time),
                    user_time: self.filetime_to_u64(user_time),
                    creation_time: self.filetime_to_u64(creation_time),
                    exit_time: self.filetime_to_u64(exit_time),
                })
            } else {
                None
            }
        }
    }

    fn get_windows_for_process(&self, pid: u32) -> Result<Vec<WindowInfo>> {
        let mut windows = Vec::new();
        let mut enum_data = WindowEnumData {
            target_pid: pid,
            windows: &mut windows,
        };

        unsafe {
            EnumWindows(
                Some(enum_windows_proc),
                &mut enum_data as *mut _ as isize,
            );
        }

        Ok(windows)
    }

    // NOUVELLE FONCTION : Détecter l'onglet actif d'un navigateur
    pub fn get_active_browser_tab(&self, pid: u32) -> Result<Option<WindowInfo>> {
        let windows = self.get_windows_for_process(pid)?;
        
        // 1. D'abord, essayer de trouver la fenêtre au premier plan
        if let Some(foreground_window) = self.get_foreground_window() {
            for window in &windows {
                if window.hwnd == foreground_window as u64 {
                    // Vérifier si c'est une fenêtre de navigateur avec un titre
                    if self.is_browser_content_window(window) && !window.window_title.is_empty() {
                        return Ok(Some(window.clone()));
                    }
                }
            }
        }
        
        // 2. Sinon, prendre la première fenêtre visible avec un titre
        for window in &windows {
            if window.is_visible && self.is_browser_content_window(window) && !window.window_title.is_empty() {
                return Ok(Some(window.clone()));
            }
        }
        
        Ok(None)
    }

    fn get_foreground_window(&self) -> Option<HWND> {
        unsafe {
            let hwnd = winapi::um::winuser::GetForegroundWindow();
            if hwnd != null_mut() {
                Some(hwnd)
            } else {
                None
            }
        }
    }

    fn is_browser_content_window(&self, window: &WindowInfo) -> bool {
        // Détecter les fenêtres de contenu de navigateur
        match window.class_name.as_str() {
            "MozillaWindowClass" |           // Firefox
            "Chrome_WidgetWin_1" |           // Chrome
            "Chrome_WidgetWin_0" |           // Chrome (ancien)
            "EdgeUiInputTopWndClass" |       // Edge
            "ApplicationFrameWindow" |       // Edge (UWP)
            "MozillaDropShadowWindowClass" => true, // Firefox (ombres)
            _ => false,
        }
    }

    fn get_threads_for_process(&self, pid: u32) -> Result<Vec<ThreadInfo>> {
        let mut threads = Vec::new();
        
        unsafe {
            let snapshot: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot != null_mut() {
                let mut te32: THREADENTRY32 = mem::zeroed();
                te32.dwSize = mem::size_of::<THREADENTRY32>() as u32;
                
                if Thread32First(snapshot, &mut te32) != 0 {
                    loop {
                        if te32.th32OwnerProcessID == pid {
                            let thread_info = ThreadInfo {
                                thread_id: te32.th32ThreadID,
                                process_id: te32.th32OwnerProcessID,
                                creation_time: None,
                                exit_time: None,
                                kernel_time: 0,
                                user_time: 0,
                                priority: 0,
                                base_priority: 0,
                                context_switches: te32.dwFlags,
                            };
                            threads.push(thread_info);
                        }

                        if Thread32Next(snapshot, &mut te32) == 0 {
                            break;
                        }
                    }
                }
                CloseHandle(snapshot);
            }
        }

        Ok(threads)
    }

    fn get_modules_for_process(&self, pid: u32) -> Result<Vec<ModuleInfo>> {
        let mut modules = Vec::new();
        
        unsafe {
            let snapshot: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid);
            if snapshot != null_mut() {
                let mut me32: MODULEENTRY32 = mem::zeroed();
                me32.dwSize = mem::size_of::<MODULEENTRY32>() as u32;
                
                if Module32First(snapshot, &mut me32) != 0 {
                    loop {
                        let module_info = ModuleInfo {
                            module_name: self.c_string_to_string_256(&me32.szModule),
                            module_path: self.c_string_to_string_260(&me32.szExePath),
                            base_address: me32.modBaseAddr as u64,
                            module_size: me32.modBaseSize,
                            entry_point: 0, // Pas disponible dans MODULEENTRY32
                        };
                        modules.push(module_info);

                        if Module32Next(snapshot, &mut me32) == 0 {
                            break;
                        }
                    }
                }
                CloseHandle(snapshot);
            }
        }

        Ok(modules)
    }

    fn get_environment_variables(&self, _pid: u32) -> Result<HashMap<String, String>> {
        let env_vars = HashMap::new();
        
        // Pour l'instant, on retourne une HashMap vide
        // TODO: Implémenter la récupération des variables d'environnement du processus
        // Cela nécessite des privilèges élevés et des APIs spécialisées
        
        Ok(env_vars)
    }

    pub fn get_process_name_by_pid(&self, pid: u32) -> String {
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

    fn filetime_to_u64(&self, ft: FILETIME) -> u64 {
        ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
    }

    fn c_string_to_string_256(&self, c_str: &[i8; 256]) -> String {
        let end = c_str.iter().position(|&x| x == 0).unwrap_or(c_str.len());
        let bytes: Vec<u8> = c_str[..end].iter().map(|&x| x as u8).collect();
        String::from_utf8_lossy(&bytes).to_string()
    }

    fn c_string_to_string_260(&self, c_str: &[i8; 260]) -> String {
        let end = c_str.iter().position(|&x| x == 0).unwrap_or(c_str.len());
        let bytes: Vec<u8> = c_str[..end].iter().map(|&x| x as u8).collect();
        String::from_utf8_lossy(&bytes).to_string()
    }
}

struct WindowEnumData<'a> {
    target_pid: u32,
    windows: &'a mut Vec<WindowInfo>,
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let enum_data = &mut *(lparam as *mut WindowEnumData);
    
    let mut process_id: DWORD = 0;
    GetWindowThreadProcessId(hwnd, &mut process_id);
    
    if process_id == enum_data.target_pid {
        let mut window_text: [u16; MAX_PATH] = [0; MAX_PATH];
        let text_len = GetWindowTextW(hwnd, window_text.as_mut_ptr(), MAX_PATH as i32);
        
        let mut class_name: [u16; MAX_PATH] = [0; MAX_PATH];
        let class_len = GetClassNameW(hwnd, class_name.as_mut_ptr(), MAX_PATH as i32);
        
        let mut window_rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let has_rect = GetWindowRect(hwnd, &mut window_rect) != 0;
        
        let title = if text_len > 0 {
            let os_string = OsString::from_wide(&window_text[..text_len as usize]);
            os_string.into_string().unwrap_or_default()
        } else {
            String::new()
        };
        
        let class = if class_len > 0 {
            let os_string = OsString::from_wide(&class_name[..class_len as usize]);
            os_string.into_string().unwrap_or_default()
        } else {
            String::new()
        };
        
        let mut thread_id: DWORD = 0;
        GetWindowThreadProcessId(hwnd, &mut thread_id);
        
        let window_info = WindowInfo {
            hwnd: hwnd as u64,
            class_name: class,
            window_title: title,
            process_id: process_id,
            thread_id: thread_id,
            is_visible: IsWindowVisible(hwnd) != 0,
            window_rect: if has_rect {
                Some(WindowRect {
                    left: window_rect.left,
                    top: window_rect.top,
                    right: window_rect.right,
                    bottom: window_rect.bottom,
                })
            } else {
                None
            },
        };
        
        enum_data.windows.push(window_info);
    }
    
    1 // Continue enumeration
}
