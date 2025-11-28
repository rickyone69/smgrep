use std::{
   fs,
   path::{Path, PathBuf},
   time::Duration,
};

use tree_sitter::{Language, Parser, WasmStore, wasmtime};

use crate::{
   config,
   error::{ChunkerError, ConfigError, Error, Result},
};

pub type GrammarPair = (&'static str, &'static str);

pub const GRAMMAR_URLS: &[GrammarPair] = &[
    ("typescript", "https://github.com/tree-sitter/tree-sitter-typescript/releases/latest/download/tree-sitter-typescript.wasm"),
    ("tsx",        "https://github.com/tree-sitter/tree-sitter-typescript/releases/latest/download/tree-sitter-tsx.wasm"),
    ("python",     "https://github.com/tree-sitter/tree-sitter-python/releases/latest/download/tree-sitter-python.wasm"),
    ("go",         "https://github.com/tree-sitter/tree-sitter-go/releases/latest/download/tree-sitter-go.wasm"),
    ("rust",       "https://github.com/tree-sitter/tree-sitter-rust/releases/latest/download/tree-sitter-rust.wasm"),
    ("javascript", "https://github.com/tree-sitter/tree-sitter-javascript/releases/latest/download/tree-sitter-javascript.wasm"),
    ("c",          "https://github.com/tree-sitter/tree-sitter-c/releases/latest/download/tree-sitter-c.wasm"),
    ("cpp",        "https://github.com/tree-sitter/tree-sitter-cpp/releases/latest/download/tree-sitter-cpp.wasm"),
    ("java",       "https://github.com/tree-sitter/tree-sitter-java/releases/latest/download/tree-sitter-java.wasm"),
    ("ruby",       "https://github.com/tree-sitter/tree-sitter-ruby/releases/latest/download/tree-sitter-ruby.wasm"),
    ("php",        "https://github.com/tree-sitter/tree-sitter-php/releases/latest/download/tree-sitter-php.wasm"),
    ("html",       "https://github.com/tree-sitter/tree-sitter-html/releases/latest/download/tree-sitter-html.wasm"),
    ("css",        "https://github.com/tree-sitter/tree-sitter-css/releases/latest/download/tree-sitter-css.wasm"),
    ("bash",       "https://github.com/tree-sitter/tree-sitter-bash/releases/latest/download/tree-sitter-bash.wasm"),
    ("json",       "https://github.com/tree-sitter/tree-sitter-json/releases/latest/download/tree-sitter-json.wasm"),
];

pub static EXTENSION_MAP: &[(&str, &str)] = &[
   ("js", "javascript"),
   ("mjs", "javascript"),
   ("cjs", "javascript"),
   ("ts", "typescript"),
   ("mts", "typescript"),
   ("cts", "typescript"),
   ("jsx", "tsx"),
   ("tsx", "tsx"),
   ("py", "python"),
   ("pyi", "python"),
   ("go", "go"),
   ("rs", "rust"),
   ("c", "c"),
   ("h", "c"),
   ("cpp", "cpp"),
   ("cc", "cpp"),
   ("cxx", "cpp"),
   ("c++", "cpp"),
   ("hpp", "cpp"),
   ("hxx", "cpp"),
   ("h++", "cpp"),
   ("java", "java"),
   ("rb", "ruby"),
   ("php", "php"),
   ("html", "html"),
   ("htm", "html"),
   ("css", "css"),
   ("sh", "bash"),
   ("bash", "bash"),
   ("json", "json"),
];

pub struct GrammarManager {
   grammar_dir:   PathBuf,
   engine:        wasmtime::Engine,
   languages:     moka::future::Cache<&'static str, Language>,
   auto_download: bool,
}

impl std::fmt::Debug for GrammarManager {
   fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      f.debug_struct("GrammarManager")
         .field("languages", &self.languages)
         .field("grammars_dir", &self.grammar_dir)
         .field("auto_download", &self.auto_download)
         .finish()
   }
}

impl GrammarManager {
   pub fn new() -> Result<Self> {
      Self::with_auto_download(true)
   }

   pub fn with_auto_download(auto_download: bool) -> Result<Self> {
      let grammar_dir = config::grammar_dir();
      fs::create_dir_all(grammar_dir).map_err(ConfigError::CreateGrammarsDir)?;

      let engine = wasmtime::Engine::default();

      Ok(Self {
         grammar_dir: grammar_dir.clone(),
         engine,
         languages: moka::future::Cache::builder()
            .time_to_idle(Duration::from_secs(5 * 60))
            .build(),
         auto_download,
      })
   }

   pub fn grammar_dir(&self) -> &Path {
      &self.grammar_dir
   }

   pub fn extension_to_language(ext: &str) -> Option<&'static str> {
      EXTENSION_MAP
         .iter()
         .find(|(e, _)| e.eq_ignore_ascii_case(ext))
         .map(|(_, lang)| *lang)
   }

   pub fn grammar_url(lang: &str) -> Option<&'static str> {
      GRAMMAR_URLS
         .iter()
         .find(|(l, _)| l.eq_ignore_ascii_case(lang))
         .map(|(_, url)| *url)
   }

   pub fn grammar_path(&self, lang: &str) -> PathBuf {
      self.grammar_dir.join(format!("tree-sitter-{lang}.wasm"))
   }

   pub fn is_available(&self, lang: &str) -> bool {
      self.grammar_path(lang).exists()
   }

   pub fn available_languages(&self) -> impl Iterator<Item = &'static str> + Clone {
      GRAMMAR_URLS
         .iter()
         .filter(|(lang, _)| self.is_available(lang))
         .map(|(lang, _)| *lang)
   }

   pub fn missing_languages(&self) -> impl Iterator<Item = &'static str> + Clone {
      GRAMMAR_URLS
         .iter()
         .filter(|(lang, _)| !self.is_available(lang))
         .map(|(lang, _)| *lang)
   }

   fn load_language(&self, lang: &str, bytes: &[u8]) -> Result<Language> {
      let mut store = WasmStore::new(&self.engine).map_err(ChunkerError::CreateWasmStore)?;
      store
         .load_language(lang, bytes)
         .map_err(|e| ChunkerError::LoadLanguage { lang: lang.to_string(), reason: e }.into())
   }

   pub async fn download_grammar(&self, pair: GrammarPair) -> Result<Language> {
      let (lang, url) = pair;
      let dest = self.grammar_path(lang);
      if dest.exists() {
         let language = fs::read(&dest)
            .map_err(Error::from)
            .and_then(|bytes| self.load_language(lang, &bytes));
         if let Ok(language) = language {
            return Ok(language);
         }
      }

      tracing::info!("downloading grammar for {} from {}", lang, url);

      let response = reqwest::get(url)
         .await
         .map_err(|e| Error::Config(ConfigError::DownloadFailed { lang, reason: e }))?;

      if !response.status().is_success() {
         return Err(Error::Config(ConfigError::DownloadHttpStatus {
            lang,
            status: response.status().as_u16(),
         }));
      }

      let bytes = response.bytes().await.map_err(ConfigError::ReadResponse)?;

      tracing::info!("downloaded grammar for {}", lang);

      let language = self.load_language(lang, &bytes)?;

      fs::write(&dest, &bytes).map_err(ConfigError::WriteWasmFile)?;

      Ok(language)
   }

   pub async fn get_language(&self, lang: &str) -> Result<Option<Language>> {
      let pair = GRAMMAR_URLS
         .iter()
         .find(|(l, _)| l.eq_ignore_ascii_case(lang));
      let Some(pair) = pair else {
         return Ok(None);
      };

      let cache = self
         .languages
         .try_get_with(pair.0, async { self.download_grammar(*pair).await })
         .await?;
      Ok(Some(cache))
   }

   pub async fn get_language_for_path(&self, path: &Path) -> Result<Option<Language>> {
      let lang = path
         .extension()
         .and_then(|e| e.to_str())
         .and_then(Self::extension_to_language);
      let Some(lang) = lang else {
         return Ok(None);
      };
      self.get_language(lang).await
   }

   pub fn create_parser_with_store(&self) -> Result<(Parser, WasmStore)> {
      let parser = Parser::new();
      let store = WasmStore::new(&self.engine).map_err(ChunkerError::CreateWasmStore)?;
      Ok((parser, store))
   }
}

impl Default for GrammarManager {
   fn default() -> Self {
      Self::new().expect("failed to create grammar manager")
   }
}
