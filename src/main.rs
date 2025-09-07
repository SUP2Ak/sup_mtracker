use sup_mtracker::ProcessScanner;
use std::fs;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() > 1 {
        let scanner = ProcessScanner::new();
        
        // Mode spécial : trouver un processus par nom d'exécutable
        if args.len() > 2 && args[2] == "find-by-name" {
            let executable_name = &args[1];
            println!("🔍 Recherche du processus: {}", executable_name);
            
            if let Some(found_pid) = scanner.find_pid_by_executable_name(executable_name)? {
                println!("✅ Processus trouvé ! PID: {}", found_pid);
                
                // Récupérer les métadonnées
                if let Some(metadata) = scanner.monitor_process_by_name(executable_name, None)? {
                    println!("📊 Métadonnées:");
                    println!("  PID: {}", metadata.pid);
                    println!("  Nom: {}", metadata.name);
                    if let Some(path) = &metadata.executable_path {
                        println!("  Chemin: {}", path);
                    }
                    if let Some(mem) = &metadata.memory_info {
                        println!("  Mémoire: {} MB", mem.working_set_size / 1024 / 1024);
                    }
                    println!("  Fenêtres: {}", metadata.windows.len());
                    if let Some(title) = &metadata.window_title {
                        println!("  Titre: {}", title);
                    }
                }
            } else {
                println!("❌ Processus '{}' non trouvé", executable_name);
            }
        } else if let Ok(pid) = args[1].parse::<u32>() {
            // Mode métadonnées pour un PID spécifique
            // Mode spécial : test de détection d'onglet actif
            if args.len() > 2 && args[2] == "active-tab" {
                println!("🔍 Test de détection d'onglet actif pour PID: {}", pid);
                
                if let Some(active_tab) = scanner.get_active_browser_tab(pid)? {
                    println!("✅ Onglet actif détecté !");
                    println!("  📱 Titre: {}", active_tab.window_title);
                    println!("  🪟 Classe: {}", active_tab.class_name);
                    println!("  👁️ Visible: {}", active_tab.is_visible);
                    if let Some(rect) = &active_tab.window_rect {
                        println!("  📐 Position: ({}, {}) - ({}, {})", rect.left, rect.top, rect.right, rect.bottom);
                    }
                } else {
                    println!("❌ Aucun onglet actif détecté");
                }
            } else {
                println!("🔍 Récupération des métadonnées pour PID: {}", pid);
                let metadata = scanner.get_process_metadata(pid, None)?;
            
                // Convertir en JSON
                let json_output = serde_json::to_string_pretty(&metadata)?;
                
                // Sauvegarder dans un fichier
                let filename = format!("process_metadata_{}.json", pid);
                fs::write(&filename, &json_output)?;
                
                println!("✅ Métadonnées récupérées !");
                println!("💾 Résultats sauvegardés dans: {}", filename);
                
                // Afficher un résumé
                println!("\n📊 Résumé des métadonnées:");
                println!("  PID: {}", metadata.pid);
                println!("  Nom: {}", metadata.name);
                if let Some(path) = &metadata.executable_path {
                    println!("  Chemin: {}", path);
                }
                if let Some(mem) = &metadata.memory_info {
                    println!("  Mémoire: {} MB", mem.working_set_size / 1024 / 1024);
                }
                println!("  Handles: {}", metadata.handle_count);
            }
            
        } else {
            println!("❌ PID invalide. Utilisez: {} <PID>", args[0]);
        }
    } else {
        // Mode scan des applications (comme avant)
        println!("🔍 Démarrage du scan des applications...");
        
        let scanner = ProcessScanner::new();
        let result = scanner.scan_applications()?;
        
        // Convertir en JSON
        let json_output = serde_json::to_string_pretty(&result)?;
        
        // Sauvegarder dans un fichier
        let filename = format!("applications_scan_{}.json", result.scan_timestamp);
        fs::write(&filename, &json_output)?;
        
        println!("✅ Scan terminé !");
        println!("📊 Applications trouvées: {}", result.total_applications);
        println!("💾 Résultats sauvegardés dans: {}", filename);
        
        // Afficher un résumé
        for app in &result.applications {
            println!(
                "  📱 {} (PID: {}) - {} processus",
                app.main_process.name,
                app.main_process.pid,
                app.total_processes
            );
            if let Some(title) = &app.main_process.window_title {
                println!("     Titre: {}", title);
            }
        }
        
        println!("\n💡 Pour récupérer les métadonnées détaillées d'un processus:");
        println!("   {} <PID>", args[0]);
    }
    
    Ok(())
}