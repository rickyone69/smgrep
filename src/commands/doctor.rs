use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use console::style;

use crate::{
   config::{COLBERT_MODEL, DENSE_MODEL},
   grammar::{GRAMMAR_URLS, GrammarManager},
};

pub async fn execute() -> Result<()> {
   println!("{}\n", style("rsgrep Doctor").bold());

   let home = directories::UserDirs::new()
      .context("failed to get user directories")?
      .home_dir()
      .to_path_buf();

   let root = home.join(".rsgrep");
   let models = root.join("models");
   let data = root.join("data");
   let grammars = root.join("grammars");

   check_dir("Root", &root);
   check_dir("Models", &models);
   check_dir("Data (Vector DB)", &data);
   check_dir("Grammars", &grammars);

   println!();

   let mut all_good = true;

   for model_id in &[DENSE_MODEL, COLBERT_MODEL] {
      let model_path = models.join(model_id.replace('/', "--"));
      let exists = model_path.exists();

      let symbol = if exists {
         style("✓").green()
      } else {
         all_good = false;
         style("✗").red()
      };

      println!(
         "{} Model: {} ({})",
         symbol,
         style(model_id).dim(),
         style(model_path.display()).dim()
      );
   }

   println!();

   let grammar_manager = match GrammarManager::with_auto_download(false) {
      Ok(gm) => Some(gm),
      Err(_) => {
         println!(
            "{} Grammar manager: {}",
            style("✗").red(),
            style("failed to initialize").dim()
         );
         all_good = false;
         None
      },
   };

   if let Some(gm) = &grammar_manager {
      let available = gm.available_languages();
      let missing = gm.missing_languages();

      for (lang, _) in GRAMMAR_URLS {
         let exists = available.contains(lang);

         let symbol = if exists {
            style("✓").green()
         } else {
            style("○").yellow()
         };

         let status = if exists {
            "installed".to_string()
         } else {
            "will download on first use".to_string()
         };

         println!("{} Grammar: {} ({})", symbol, style(lang).dim(), style(status).dim());
      }

      println!();
      println!(
         "{} {} of {} grammars installed",
         style("ℹ").cyan(),
         available.len(),
         GRAMMAR_URLS.len()
      );
      if !missing.is_empty() {
         println!(
            "{} Missing grammars will be downloaded automatically when needed",
            style("ℹ").cyan()
         );
      }
   }

   if data.exists()
      && let Ok(size) = get_dir_size(&data)
   {
      println!("\n{} {}", style("Data directory size:").dim(), style(format_size(size)).cyan());
   }

   println!(
      "\n{} {} {} | Rust: {}",
      style("System:").dim(),
      std::env::consts::OS,
      std::env::consts::ARCH,
      rustc_version_runtime::version()
   );

   if all_good {
      println!(
         "\n{}",
         style("✓ All checks passed! You are ready to grep.")
            .green()
            .bold()
      );
   } else {
      println!(
         "\n{}",
         style("✗ Some components are missing. Run 'rsgrep setup' to download them.")
            .red()
            .bold()
      );
   }

   Ok(())
}

fn check_dir(name: &str, path: &PathBuf) {
   let exists = path.exists();
   let symbol = if exists {
      style("✓").green()
   } else {
      style("✗").red()
   };
   println!("{} {}: {}", symbol, name, style(path.display()).dim());
}

fn get_dir_size(path: &PathBuf) -> Result<u64> {
   let mut total = 0u64;

   if path.is_dir() {
      for entry in fs::read_dir(path)? {
         let entry = entry?;
         let metadata = entry.metadata()?;

         if metadata.is_dir() {
            total += get_dir_size(&entry.path())?;
         } else {
            total += metadata.len();
         }
      }
   }

   Ok(total)
}

fn format_size(bytes: u64) -> String {
   const KB: u64 = 1024;
   const MB: u64 = KB * 1024;
   const GB: u64 = MB * 1024;

   if bytes < KB {
      format!("{} B", bytes)
   } else if bytes < MB {
      format!("{:.1} KB", bytes as f64 / KB as f64)
   } else if bytes < GB {
      format!("{:.1} MB", bytes as f64 / MB as f64)
   } else {
      format!("{:.1} GB", bytes as f64 / GB as f64)
   }
}
