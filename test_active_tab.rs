use sup_mtracker::ProcessScanner;
use anyhow::Result;

fn main() -> Result<()> {
    let scanner = ProcessScanner::new();
    
    // Test avec le processus Firefox principal (23664)
    println!("🔍 Test de détection d'onglet actif pour Firefox (PID: 23664)");
    
    if let Some(active_tab) = scanner.get_active_browser_tab(23664)? {
        println!("✅ Onglet actif détecté !");
        println!("  📱 Titre: {}", active_tab.window_title);
        println!("  🪟 Classe: {}", active_tab.class_name);
        println!("  👁️ Visible: {}", active_tab.is_visible);
        println!("  📐 Position: ({}, {}) - ({}, {})", 
                 active_tab.window_rect.as_ref().map(|r| r.left).unwrap_or(0),
                 active_tab.window_rect.as_ref().map(|r| r.top).unwrap_or(0),
                 active_tab.window_rect.as_ref().map(|r| r.right).unwrap_or(0),
                 active_tab.window_rect.as_ref().map(|r| r.bottom).unwrap_or(0));
    } else {
        println!("❌ Aucun onglet actif détecté");
    }
    
    Ok(())
}
